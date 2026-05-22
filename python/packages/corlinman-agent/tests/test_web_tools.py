"""Tests for the builtin web tools (``web_fetch`` / ``web_search``) and
the self-contained ``calculator``.

Network is mocked with :class:`httpx.MockTransport` — no real I/O, no
new test dependency (``respx`` is not in the dependency set; the rerank
client tests set the same precedent).
"""

from __future__ import annotations

import asyncio
import json
from typing import Callable

import httpx
import pytest
from corlinman_agent.web import (
    CALCULATOR_TOOL,
    WEB_FETCH_TOOL,
    WEB_SEARCH_TOOL,
    calculator_tool_schema,
    dispatch_calculator,
    dispatch_web_fetch,
    dispatch_web_search,
    web_fetch_tool_schema,
    web_search_tool_schema,
)
from corlinman_agent.web.fetch import DEFAULT_MAX_CHARS, MAX_BODY_BYTES


def _transport(
    handler: Callable[[httpx.Request], httpx.Response],
) -> httpx.MockTransport:
    return httpx.MockTransport(handler)


# ---------------------------------------------------------------------------
# Schemas / wire-stable names
# ---------------------------------------------------------------------------


def test_tool_names_are_wire_stable() -> None:
    assert WEB_FETCH_TOOL == "web_fetch"
    assert WEB_SEARCH_TOOL == "web_search"
    assert CALCULATOR_TOOL == "calculator"


@pytest.mark.parametrize(
    ("schema_fn", "name"),
    [
        (web_fetch_tool_schema, "web_fetch"),
        (web_search_tool_schema, "web_search"),
        (calculator_tool_schema, "calculator"),
    ],
)
def test_schemas_are_openai_shaped(schema_fn, name) -> None:  # type: ignore[no-untyped-def]
    schema = schema_fn()
    assert schema["type"] == "function"
    assert schema["function"]["name"] == name
    assert "parameters" in schema["function"]
    assert schema["function"]["parameters"]["type"] == "object"


# ---------------------------------------------------------------------------
# web_fetch
# ---------------------------------------------------------------------------


def test_web_fetch_success_strips_html() -> None:
    html_body = (
        "<html><head><title>Hello Page</title>"
        "<style>.x{color:red}</style></head>"
        "<body><script>var a=1;</script>"
        "<h1>Heading</h1><p>First &amp; paragraph.</p>"
        "<p>Second paragraph.</p></body></html>"
    )

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200, html=html_body, headers={"content-type": "text/html"}
        )

    out = json.loads(
        asyncio.run(
            dispatch_web_fetch(
                args_json=json.dumps({"url": "https://example.com/doc"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["status"] == 200
    assert out["title"] == "Hello Page"
    assert "First & paragraph." in out["text"]
    assert "Second paragraph." in out["text"]
    # script / style content must be gone.
    assert "var a=1" not in out["text"]
    assert "color:red" not in out["text"]
    assert out["truncated"] is False


def test_web_fetch_plain_text_passthrough() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            text="just plain text",
            headers={"content-type": "text/plain"},
        )

    out = json.loads(
        asyncio.run(
            dispatch_web_fetch(
                args_json=json.dumps({"url": "https://example.com/raw.txt"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["text"] == "just plain text"
    assert out["title"] is None


def test_web_fetch_timeout() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        raise httpx.ConnectTimeout("connect timed out", request=request)

    out = json.loads(
        asyncio.run(
            dispatch_web_fetch(
                args_json=json.dumps({"url": "https://slow.example.com"}),
                transport=_transport(handler),
            )
        )
    )
    assert "error" in out
    assert out["error"].startswith("timeout:")


def test_web_fetch_non_200() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(404, text="<html>not found</html>")

    out = json.loads(
        asyncio.run(
            dispatch_web_fetch(
                args_json=json.dumps({"url": "https://example.com/missing"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["status"] == 404
    assert out["error"].startswith("http_status:")


def test_web_fetch_oversized_body_is_truncated() -> None:
    # Body larger than MAX_BODY_BYTES — must be flagged truncated and
    # never blow memory (bounded mid-stream).
    big = "x" * (MAX_BODY_BYTES + 5_000)

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200, text=big, headers={"content-type": "text/plain"}
        )

    out = json.loads(
        asyncio.run(
            dispatch_web_fetch(
                args_json=json.dumps({"url": "https://example.com/big"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["truncated"] is True
    assert len(out["text"]) <= DEFAULT_MAX_CHARS


def test_web_fetch_respects_max_chars() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200, text="abcdefghij" * 50, headers={"content-type": "text/plain"}
        )

    out = json.loads(
        asyncio.run(
            dispatch_web_fetch(
                args_json=json.dumps(
                    {"url": "https://example.com/x", "max_chars": 25}
                ),
                transport=_transport(handler),
            )
        )
    )
    assert len(out["text"]) == 25
    assert out["truncated"] is True


def test_web_fetch_rejects_bad_url() -> None:
    out = json.loads(
        asyncio.run(dispatch_web_fetch(args_json=json.dumps({"url": "ftp://x"})))
    )
    assert out["error"].startswith("args_invalid:")


def test_web_fetch_rejects_missing_url() -> None:
    out = json.loads(asyncio.run(dispatch_web_fetch(args_json=b"{}")))
    assert out["error"].startswith("args_invalid:")


def test_web_fetch_rejects_bad_json() -> None:
    out = json.loads(asyncio.run(dispatch_web_fetch(args_json=b"not json")))
    assert out["error"].startswith("args_invalid:")


# ---------------------------------------------------------------------------
# web_search
# ---------------------------------------------------------------------------

_DDG_HTML = """
<html><body>
<div class="result">
  <a class="result__a" href="https://a.example.com/page">First &amp; Result</a>
  <a class="result__snippet">Snippet about the first result.</a>
</div>
<div class="result">
  <a class="result__a"
     href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fb.example.com%2Fx&amp;rut=z">
     Second Result</a>
  <a class="result__snippet">Snippet <b>two</b> here.</a>
</div>
</body></html>
"""


def test_web_search_parses_ddg_results(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_BACKEND", raising=False)
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_API_KEY", raising=False)

    def handler(request: httpx.Request) -> httpx.Response:
        assert "duckduckgo.com" in str(request.url)
        return httpx.Response(200, text=_DDG_HTML)

    out = json.loads(
        asyncio.run(
            dispatch_web_search(
                args_json=json.dumps({"query": "corlinman agent"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["backend"] == "ddg"
    assert len(out["results"]) == 2
    first = out["results"][0]
    assert first["title"] == "First & Result"
    assert first["url"] == "https://a.example.com/page"
    assert "first result" in first["snippet"].lower()
    # redirect-wrapped URL must be unwrapped to the real target.
    assert out["results"][1]["url"] == "https://b.example.com/x"


def test_web_search_respects_max_results(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_BACKEND", raising=False)
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_API_KEY", raising=False)

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(200, text=_DDG_HTML)

    out = json.loads(
        asyncio.run(
            dispatch_web_search(
                args_json=json.dumps({"query": "x", "max_results": 1}),
                transport=_transport(handler),
            )
        )
    )
    assert len(out["results"]) == 1


def test_web_search_degrades_on_http_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_BACKEND", raising=False)
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_API_KEY", raising=False)

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(503, text="service unavailable")

    out = json.loads(
        asyncio.run(
            dispatch_web_search(
                args_json=json.dumps({"query": "x"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["results"] == []
    assert out["error"].startswith("search_unavailable:")


def test_web_search_degrades_on_timeout(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_BACKEND", raising=False)
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_API_KEY", raising=False)

    def handler(request: httpx.Request) -> httpx.Response:
        raise httpx.ReadTimeout("read timed out", request=request)

    out = json.loads(
        asyncio.run(
            dispatch_web_search(
                args_json=json.dumps({"query": "x"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["results"] == []
    assert out["error"].startswith("timeout:")


def test_web_search_rejects_missing_query() -> None:
    out = json.loads(asyncio.run(dispatch_web_search(args_json=b"{}")))
    assert out["results"] == []
    assert out["error"].startswith("args_invalid:")


def test_web_search_serpapi_backend(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("CORLINMAN_WEB_SEARCH_API_KEY", "secret-key")
    monkeypatch.delenv("CORLINMAN_WEB_SEARCH_BACKEND", raising=False)

    def handler(request: httpx.Request) -> httpx.Response:
        assert "serpapi.com" in str(request.url)
        assert "secret-key" in str(request.url)
        return httpx.Response(
            200,
            json={
                "organic_results": [
                    {
                        "title": "SerpApi Hit",
                        "link": "https://c.example.com",
                        "snippet": "from serpapi",
                    }
                ]
            },
        )

    out = json.loads(
        asyncio.run(
            dispatch_web_search(
                args_json=json.dumps({"query": "x"}),
                transport=_transport(handler),
            )
        )
    )
    assert out["backend"] == "serpapi"
    assert out["results"][0]["url"] == "https://c.example.com"


def test_web_search_unknown_backend_degrades(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("CORLINMAN_WEB_SEARCH_BACKEND", "bing-nope")

    out = json.loads(
        asyncio.run(dispatch_web_search(args_json=json.dumps({"query": "x"})))
    )
    assert out["results"] == []
    assert out["error"].startswith("unknown_backend:")


# ---------------------------------------------------------------------------
# calculator
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("expr", "expected"),
    [
        ("2 + 2", 4),
        ("2 + 3 * 4", 14),
        ("(1234 * 5678) / 2", 3503326.0),
        ("2 ** 10", 1024),
        ("17 % 5", 2),
        ("17 // 5", 3),
        ("-(3 + 4)", -7),
    ],
)
def test_calculator_evaluates(expr: str, expected: float) -> None:
    out = json.loads(dispatch_calculator(args_json=json.dumps({"expression": expr})))
    assert out["result"] == expected


def test_calculator_division_by_zero() -> None:
    out = json.loads(
        dispatch_calculator(args_json=json.dumps({"expression": "1 / 0"}))
    )
    assert out["error"] == "division by zero"


def test_calculator_rejects_code_injection() -> None:
    for evil in ["__import__('os')", "open('x')", "x + 1", "[i for i in range(3)]"]:
        out = json.loads(
            dispatch_calculator(args_json=json.dumps({"expression": evil}))
        )
        assert "error" in out
        assert "result" not in out


def test_calculator_rejects_huge_exponent() -> None:
    out = json.loads(
        dispatch_calculator(args_json=json.dumps({"expression": "9 ** 999999"}))
    )
    assert out["error"].startswith("invalid_expression:")


def test_calculator_rejects_missing_expression() -> None:
    out = json.loads(dispatch_calculator(args_json=b"{}"))
    assert out["error"].startswith("args_invalid:")


def test_calculator_rejects_bad_json() -> None:
    out = json.loads(dispatch_calculator(args_json=b"<<<"))
    assert out["error"].startswith("args_invalid:")
