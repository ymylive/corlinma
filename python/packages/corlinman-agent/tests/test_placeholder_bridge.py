"""Tests for the Python → Rust placeholder gRPC bridge.

B2-BE4 moved the message-preprocessing pipeline into
:class:`ContextAssembler`; the assembly-level tests now live in
``test_context_assembler.py``. This file keeps the narrow bridge
contract:

* System-inject prefix detection.
* :class:`PlaceholderClient` error decoding.
* UDS socket-path resolution.
* An opt-in integration test that spins a fake gRPC server in-process
  and round-trips a real ``Render`` request.
"""

from __future__ import annotations

import asyncio
import contextlib
import os
import tempfile

import pytest
from corlinman_agent.context_assembler import has_system_inject_prefix
from corlinman_agent.placeholder_client import (
    PlaceholderClient,
    PlaceholderCycleError,
    PlaceholderDepthError,
    PlaceholderError,
    PlaceholderResolverError,
    resolve_uds_path,
)
from corlinman_grpc import placeholder_pb2, placeholder_pb2_grpc


def test_system_inject_prefix_detection() -> None:
    assert has_system_inject_prefix("[系统提示:] hello")
    assert has_system_inject_prefix("   [系统邀请指令:] go")
    assert not has_system_inject_prefix("hello world")
    assert not has_system_inject_prefix("")


# --------------------------------------------------------------------------- #
# PlaceholderClient error mapping — no network, just decoding                  #
# --------------------------------------------------------------------------- #


def test_resolve_uds_path_explicit_beats_env(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("CORLINMAN_UDS_PATH", "/tmp/from-env.sock")
    assert resolve_uds_path("/tmp/from-arg.sock") == "/tmp/from-arg.sock"
    monkeypatch.delenv("CORLINMAN_UDS_PATH", raising=False)
    assert resolve_uds_path().endswith("/corlinman.sock")


def test_error_decoder_round_trips() -> None:
    # Inline decoder exercise via the client's private helper.
    from corlinman_agent.placeholder_client import _decode_error

    assert isinstance(_decode_error("cycle:var.x"), PlaceholderCycleError)
    assert _decode_error("cycle:var.x").key == "var.x"
    assert isinstance(_decode_error("depth_exceeded"), PlaceholderDepthError)
    assert isinstance(_decode_error("resolver:boom"), PlaceholderResolverError)
    assert _decode_error("resolver:boom").detail == "boom"
    # Unknown shape still raises the base class.
    assert isinstance(_decode_error("weird"), PlaceholderError)


# --------------------------------------------------------------------------- #
# Integration — fake gRPC server in-process                                    #
# --------------------------------------------------------------------------- #


class _ScriptedPlaceholderServicer(placeholder_pb2_grpc.PlaceholderServicer):
    """Echoes a canned response mapping input template → rendered string.

    Uses a very small rule set: substitutions are preloaded, and any
    tokens that don't match a substitution are returned verbatim in
    ``unresolved_keys`` so the test can assert the round-trip shape.
    """

    def __init__(self, substitutions: dict[str, str]) -> None:
        self._subs = substitutions

    async def Render(self, request, context):  # noqa: N802 — gRPC signature
        out = request.template
        unresolved: list[str] = []
        for key, value in self._subs.items():
            out = out.replace("{{" + key + "}}", value)
        # Naively flag any token still in the output as unresolved.
        import re

        for m in re.finditer(r"\{\{([^{}]+?)\}\}", out):
            body = m.group(1).strip()
            if body and body not in unresolved:
                unresolved.append(body)
        return placeholder_pb2.RenderResponse(
            rendered=out,
            unresolved_keys=unresolved,
            error="",
        )


@pytest.fixture
def uds_path():
    """Allocate a short-lived UDS path under ``$TMPDIR``."""
    # Not using NamedTemporaryFile — we need the path to *not* exist at
    # bind time on Linux; macOS tolerates either.
    tmp = tempfile.mkdtemp(prefix="corlinman-pbtest-")
    path = os.path.join(tmp, "ph.sock")
    try:
        yield path
    finally:
        with contextlib.suppress(OSError):
            os.remove(path)
        with contextlib.suppress(OSError):
            os.rmdir(tmp)


@pytest.mark.integration
@pytest.mark.asyncio
async def test_client_round_trip_against_fake_server(uds_path: str) -> None:
    """Happy-path: the client dials a real grpc.aio server, sends a
    ``RenderRequest`` with metadata, and unpacks the response. Skipped
    when the environment can't bind a UDS socket."""
    import grpc

    server = grpc.aio.server()
    placeholder_pb2_grpc.add_PlaceholderServicer_to_server(
        _ScriptedPlaceholderServicer({"var.user_id": "u-42"}),
        server,
    )
    try:
        server.add_insecure_port(f"unix:{uds_path}")
    except Exception as exc:  # pragma: no cover — env-dependent
        pytest.skip(f"cannot bind UDS server: {exc}")
    await server.start()
    try:
        client = PlaceholderClient(uds_path=uds_path)
        try:
            result = await client.render(
                template="hi {{var.user_id}} — {{var.unknown}}",
                session_key="sess-int",
                model_name="test-model",
                metadata={"trace": "t-1"},
            )
        finally:
            await client.close()
        assert result.rendered == "hi u-42 — {{var.unknown}}"
        assert result.unresolved_keys == ["var.unknown"]
    finally:
        await server.stop(grace=None)
        # Wait for the listener to release the socket path so the
        # fixture's cleanup doesn't race with the server shutdown.
        await asyncio.sleep(0)
