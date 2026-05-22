"""``web_fetch`` builtin tool — fetch a URL, return readable text.

Given a URL, this fetches the page with :mod:`httpx` and returns the
extracted prose (HTML stripped) capped at a configurable byte budget so
a single fetch can never blow the model's context window.

Wire contract (identical to the subagent / blackboard tools):

* :data:`WEB_FETCH_TOOL` — the wire-stable tool name.
* :func:`web_fetch_tool_schema` — the OpenAI tool descriptor a parent
  drops into ``ChatStart.tools``.
* :func:`dispatch_web_fetch` — async dispatcher, ``args_json -> str``,
  never raises.

Success envelope::

    {"url": "...", "final_url": "...", "status": 200,
     "title": "...", "content_type": "text/html",
     "text": "...", "truncated": false, "bytes": 1234}

Failure envelope::

    {"url": "...", "error": "timeout: ..."}
"""

from __future__ import annotations

import json
from typing import Any

import httpx
import structlog

from corlinman_agent.web._common import (
    WebArgsInvalidError,
    decode_args,
    extract_title,
    html_to_text,
    looks_like_html,
    make_client,
)

logger = structlog.get_logger(__name__)

#: Wire-stable tool name. Imported by the gateway dispatcher's
#: ``BUILTIN_TOOLS`` set and any agent card that exposes the tool.
WEB_FETCH_TOOL: str = "web_fetch"

#: Hard ceiling on the *extracted text* returned to the model, chars.
#: ~12k chars ≈ 3k tokens — generous for a single page, bounded enough
#: that the reasoning loop's context stays sane.
DEFAULT_MAX_CHARS: int = 12_000

#: Hard ceiling on the raw response body we will buffer, bytes. A
#: response larger than this is truncated mid-stream and flagged — we
#: never load an unbounded body into memory.
MAX_BODY_BYTES: int = 4_000_000


def web_fetch_tool_schema() -> dict[str, Any]:
    """OpenAI-shaped tool descriptor for ``web_fetch``."""
    return {
        "type": "function",
        "function": {
            "name": WEB_FETCH_TOOL,
            "description": (
                "Fetch a web page (or plain-text/JSON resource) by URL "
                "and return its readable text content with HTML stripped. "
                "Use this to read documentation, articles, or API output "
                "the user references. The result is capped in length; "
                "request a specific page rather than a site root when "
                "possible."
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": (
                            "Absolute http(s) URL to fetch."
                        ),
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": (
                            "Optional cap on returned text length "
                            f"(default {DEFAULT_MAX_CHARS}, "
                            f"max {DEFAULT_MAX_CHARS})."
                        ),
                    },
                },
                "required": ["url"],
                "additionalProperties": False,
            },
        },
    }


def _parse_args(args_json: bytes | str) -> tuple[str, int]:
    raw = decode_args(args_json)
    url = raw.get("url")
    if not isinstance(url, str) or not url.strip():
        raise WebArgsInvalidError("missing or empty 'url' field")
    url = url.strip()
    scheme = url.split("://", 1)[0].lower() if "://" in url else ""
    if scheme not in ("http", "https"):
        raise WebArgsInvalidError(
            "'url' must be an absolute http(s) URL"
        )
    max_chars = raw.get("max_chars", DEFAULT_MAX_CHARS)
    if not isinstance(max_chars, int) or isinstance(max_chars, bool):
        raise WebArgsInvalidError("'max_chars' must be an integer")
    if max_chars <= 0:
        raise WebArgsInvalidError("'max_chars' must be positive")
    max_chars = min(max_chars, DEFAULT_MAX_CHARS)
    return url, max_chars


async def dispatch_web_fetch(
    *,
    args_json: bytes | str,
    transport: httpx.BaseTransport | None = None,
) -> str:
    """Translate one ``web_fetch`` tool call into a JSON envelope.

    Parameters
    ----------
    args_json
        Raw ``ToolCallEvent.args_json`` bytes (or decoded string).
    transport
        Test seam — an :class:`httpx.MockTransport` in unit tests,
        ``None`` in production (real network).

    Returns
    -------
    str
        JSON string for ``ToolResult.content``. Always returns; never
        raises — every failure path becomes ``{"error": "..."}``.
    """
    try:
        url, max_chars = _parse_args(args_json)
    except WebArgsInvalidError as exc:
        return json.dumps({"error": f"args_invalid: {exc.message}"})

    try:
        async with make_client(transport=transport) as client:
            async with client.stream("GET", url) as response:
                final_url = str(response.url)
                content_type = response.headers.get("content-type")
                # Stream the body so an oversized response is bounded.
                chunks: list[bytes] = []
                total = 0
                oversized = False
                async for chunk in response.aiter_bytes():
                    chunks.append(chunk)
                    total += len(chunk)
                    if total > MAX_BODY_BYTES:
                        oversized = True
                        break
                body_bytes = b"".join(chunks)

                if response.status_code >= 400:
                    logger.info(
                        "web_fetch.non_200",
                        url=url,
                        status=response.status_code,
                    )
                    return json.dumps(
                        {
                            "url": url,
                            "final_url": final_url,
                            "status": response.status_code,
                            "error": (
                                f"http_status: server returned "
                                f"{response.status_code}"
                            ),
                        }
                    )

        raw_text = body_bytes.decode("utf-8", errors="replace")
        if looks_like_html(content_type, raw_text):
            title = extract_title(raw_text)
            text = html_to_text(raw_text)
        else:
            title = None
            text = raw_text.strip()

        truncated = oversized or len(text) > max_chars
        if len(text) > max_chars:
            text = text[:max_chars]

        return json.dumps(
            {
                "url": url,
                "final_url": final_url,
                "status": response.status_code,
                "title": title,
                "content_type": content_type,
                "text": text,
                "truncated": truncated,
                "bytes": total,
            }
        )
    except httpx.TimeoutException as exc:
        logger.info("web_fetch.timeout", url=url, error=str(exc))
        return json.dumps({"url": url, "error": f"timeout: {exc}"})
    except httpx.HTTPError as exc:
        logger.info("web_fetch.http_error", url=url, error=str(exc))
        return json.dumps({"url": url, "error": f"fetch_failed: {exc}"})
    except Exception as exc:  # noqa: BLE001 - dispatcher must never raise
        logger.exception("web_fetch.unexpected", url=url)
        return json.dumps({"url": url, "error": f"fetch_failed: {exc}"})
