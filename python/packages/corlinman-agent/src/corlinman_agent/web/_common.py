"""Shared internals for the builtin web tools.

Kept private (leading underscore) — the public surface is the
``dispatch_*`` callables in :mod:`.fetch` / :mod:`.search` /
:mod:`.calculator`. This module holds the bits all three need: a
lenient ``args_json`` decoder mirroring the blackboard tool's, and a
dependency-free HTML → readable-text extractor.
"""

from __future__ import annotations

import html
import re
from typing import Any

import httpx

#: Default User-Agent. Some endpoints (notably DuckDuckGo) reject the
#: stock ``python-httpx`` UA, so we present as a desktop browser.
DEFAULT_USER_AGENT: str = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
    "AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/124.0.0.0 Safari/537.36"
)

#: Wall-clock ceiling for any single outbound request, seconds.
DEFAULT_TIMEOUT_SECONDS: float = 12.0


class WebArgsInvalidError(Exception):
    """Raised by the per-tool arg parsers; the dispatcher catches it and
    folds the message into an ``{"error": "args_invalid: ..."}`` envelope.
    Same shape as the subagent / blackboard ``_ArgsInvalidError`` so the
    model sees a uniform failure surface across all builtin tools."""

    def __init__(self, message: str) -> None:
        super().__init__(message)
        self.message = message


def decode_args(args_json: bytes | str) -> dict[str, Any]:
    """Decode a tool call's raw ``args_json`` into a dict.

    Accepts the ``ToolCallEvent.args_json`` bytes (utf-8 OpenAI
    ``function.arguments`` string) or an already-decoded string. Mirrors
    :func:`corlinman_agent.subagent.blackboard._decode`.
    """
    if isinstance(args_json, (bytes, bytearray)):
        try:
            decoded = bytes(args_json).decode("utf-8")
        except UnicodeDecodeError as exc:  # pragma: no cover - defensive
            raise WebArgsInvalidError(f"args_json not utf-8: {exc}") from exc
    else:
        decoded = args_json
    import json

    try:
        raw = json.loads(decoded) if decoded.strip() else {}
    except json.JSONDecodeError as exc:
        raise WebArgsInvalidError(f"args_json not JSON: {exc}") from exc
    if not isinstance(raw, dict):
        raise WebArgsInvalidError(
            f"args_json must be a JSON object, got {type(raw).__name__}"
        )
    return raw


# ---------------------------------------------------------------------------
# HTML → text
# ---------------------------------------------------------------------------

#: Block-level tags whose boundaries should become newlines so the
#: extracted text keeps a sane paragraph structure.
_BLOCK_TAGS = (
    "p",
    "div",
    "br",
    "li",
    "tr",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "header",
    "footer",
    "blockquote",
)

_SCRIPT_STYLE_RE = re.compile(
    r"<(script|style|noscript|template|svg|head)\b[^>]*>.*?</\1>",
    re.IGNORECASE | re.DOTALL,
)
_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
_BLOCK_RE = re.compile(
    r"</?(?:" + "|".join(_BLOCK_TAGS) + r")\b[^>]*>",
    re.IGNORECASE,
)
_TAG_RE = re.compile(r"<[^>]+>")
_WS_RUN_RE = re.compile(r"[ \t]+")
_BLANKLINE_RUN_RE = re.compile(r"\n\s*\n\s*")
_TITLE_RE = re.compile(r"<title\b[^>]*>(.*?)</title>", re.IGNORECASE | re.DOTALL)


def extract_title(markup: str) -> str | None:
    """Pull the ``<title>`` text out of an HTML document, if present."""
    match = _TITLE_RE.search(markup)
    if match is None:
        return None
    title = html.unescape(_TAG_RE.sub("", match.group(1))).strip()
    return title or None


def html_to_text(markup: str) -> str:
    """Strip HTML markup down to readable plain text.

    Dependency-free on purpose — ``corlinman-agent`` should not grow a
    BeautifulSoup / lxml dependency for a builtin tool. The heuristic:

    1. drop ``<script>`` / ``<style>`` / ``<head>`` / comment blocks
       wholesale (their text is never reader content);
    2. turn block-level tag boundaries into newlines so paragraphs
       survive;
    3. strip every remaining tag;
    4. unescape HTML entities and collapse whitespace runs.

    Good enough for feeding a page's prose to an LLM; it is explicitly
    *not* a layout-faithful renderer.
    """
    text = _SCRIPT_STYLE_RE.sub(" ", markup)
    text = _COMMENT_RE.sub(" ", text)
    text = _BLOCK_RE.sub("\n", text)
    text = _TAG_RE.sub("", text)
    text = html.unescape(text)
    text = _WS_RUN_RE.sub(" ", text)
    text = _BLANKLINE_RUN_RE.sub("\n\n", text)
    # Trim trailing spaces left on each line.
    text = "\n".join(line.strip() for line in text.splitlines())
    return text.strip()


def looks_like_html(content_type: str | None, body: str) -> bool:
    """Best-effort: should ``body`` be run through :func:`html_to_text`?"""
    if content_type and "html" in content_type.lower():
        return True
    if content_type and any(
        kind in content_type.lower()
        for kind in ("json", "xml", "text/plain", "csv", "javascript")
    ):
        return False
    # No / generic content-type — sniff for a tag.
    head = body.lstrip()[:512].lower()
    return head.startswith("<!doctype html") or "<html" in head or "<body" in head


def make_client(
    *,
    transport: httpx.BaseTransport | None = None,
    timeout: float = DEFAULT_TIMEOUT_SECONDS,
) -> httpx.AsyncClient:
    """Construct the outbound :class:`httpx.AsyncClient`.

    ``transport`` is the test seam — production passes ``None`` (real
    network), unit tests inject an :class:`httpx.MockTransport`.
    """
    return httpx.AsyncClient(
        transport=transport,
        timeout=timeout,
        follow_redirects=True,
        headers={"User-Agent": DEFAULT_USER_AGENT},
    )
