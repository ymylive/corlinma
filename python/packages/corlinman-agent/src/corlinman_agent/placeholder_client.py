"""Async gRPC client for the Rust-hosted ``Placeholder`` service.

The Rust gateway owns a ``PlaceholderEngine`` (``corlinman-core::placeholder``)
that resolves ``{{namespace.name}}`` tokens via a registry of async
resolvers. This client is the Python half of the bridge: the
``context_assembler`` dials it for every template that needs expansion
before the provider call.

Socket contract
---------------

The path is resolved (in order) from:

1. The ``uds_path`` constructor argument.
2. The ``CORLINMAN_UDS_PATH`` environment variable.
3. The default ``/tmp/corlinman.sock``.

Mirrors :const:`corlinman_gateway::grpc::placeholder::ENV_RUST_SOCKET` /
``DEFAULT_RUST_SOCKET`` on the Rust side.

Error shape
-----------

``RenderResponse.error`` is a stable string contract (see
``proto/corlinman/v1/placeholder.proto``):

* ``""`` — ok.
* ``"cycle:<key>"`` — :class:`PlaceholderCycleError`.
* ``"depth_exceeded"`` — :class:`PlaceholderDepthError`.
* ``"resolver:<msg>"`` — :class:`PlaceholderResolverError`.

Every other non-empty string also raises :class:`PlaceholderError` so
the caller never sees a partially-populated response silently pass as
success.
"""

from __future__ import annotations

import os
from collections.abc import Mapping

import grpc
import structlog
from corlinman_grpc import placeholder_pb2, placeholder_pb2_grpc

logger = structlog.get_logger(__name__)

_DEFAULT_SOCKET = "/tmp/corlinman.sock"
_ENV_SOCKET = "CORLINMAN_UDS_PATH"


class PlaceholderError(RuntimeError):
    """Base class for every server-reported rendering failure."""


class PlaceholderCycleError(PlaceholderError):
    """A placeholder key re-appeared while already being resolved."""

    def __init__(self, key: str) -> None:
        super().__init__(f"placeholder cycle at '{key}'")
        self.key = key


class PlaceholderDepthError(PlaceholderError):
    """Recursive expansion exceeded the engine's max_depth."""


class PlaceholderResolverError(PlaceholderError):
    """A namespace resolver raised an error on the Rust side."""

    def __init__(self, detail: str) -> None:
        super().__init__(f"resolver failed: {detail}")
        self.detail = detail


class RenderResult:
    """Structured `Render` response — the rendered template plus the
    set of tokens that had no resolver (left as literal ``{{…}}``).

    Kept as a plain class (not a dataclass) to avoid a dependency on
    dataclasses in callers that already construct ``tuple``-like payloads.
    """

    __slots__ = ("rendered", "unresolved_keys")

    def __init__(self, rendered: str, unresolved_keys: list[str]) -> None:
        self.rendered = rendered
        self.unresolved_keys = list(unresolved_keys)

    def __repr__(self) -> str:  # pragma: no cover — debug only
        return f"RenderResult(rendered={self.rendered!r}, unresolved_keys={self.unresolved_keys!r})"


def resolve_uds_path(explicit: str | None = None) -> str:
    """Socket-path lookup mirroring the Rust side."""
    if explicit:
        return explicit
    return os.environ.get(_ENV_SOCKET) or _DEFAULT_SOCKET


class PlaceholderClient:
    """Thin asyncio-friendly wrapper around the generated gRPC stub.

    One channel per instance; safe to share a single client across many
    concurrent ``render`` calls (grpc.aio multiplexes over the same UDS).
    """

    def __init__(self, uds_path: str | None = None) -> None:
        path = resolve_uds_path(uds_path)
        # grpc.aio encodes the UDS target as `unix:<path>` (no `//`);
        # absolute paths work identically on Linux + macOS.
        self._target = f"unix:{path}"
        self._channel = grpc.aio.insecure_channel(self._target)
        self._stub = placeholder_pb2_grpc.PlaceholderStub(self._channel)

    @property
    def target(self) -> str:
        """gRPC target string in use (useful for logs / tests)."""
        return self._target

    async def render(
        self,
        template: str,
        session_key: str,
        model_name: str = "",
        metadata: Mapping[str, str] | None = None,
        max_depth: int = 0,
    ) -> RenderResult:
        """Render ``template``; raise on server-reported failure."""
        ctx = placeholder_pb2.PlaceholderCtx(
            session_key=session_key,
            model_name=model_name,
            metadata=dict(metadata or {}),
        )
        req = placeholder_pb2.RenderRequest(
            template=template,
            ctx=ctx,
            max_depth=max_depth,
        )
        resp = await self._stub.Render(req)
        if resp.error:
            raise _decode_error(resp.error)
        return RenderResult(rendered=resp.rendered, unresolved_keys=list(resp.unresolved_keys))

    async def close(self) -> None:
        """Close the underlying gRPC channel.

        Idempotent: double-close swallows ``RpcError`` so callers can
        always wrap this in an ``async with`` / ``finally``.
        """
        try:
            await self._channel.close()
        except Exception as exc:  # pragma: no cover — defensive
            logger.debug("placeholder_client.close_ignored", error=str(exc))

    async def __aenter__(self) -> PlaceholderClient:
        return self

    async def __aexit__(self, *_exc: object) -> None:
        await self.close()


def _decode_error(error_str: str) -> PlaceholderError:
    """Map the wire-format error string back to a typed exception."""
    if error_str.startswith("cycle:"):
        return PlaceholderCycleError(error_str[len("cycle:"):])
    if error_str == "depth_exceeded":
        return PlaceholderDepthError("recursion depth exceeded")
    if error_str.startswith("resolver:"):
        return PlaceholderResolverError(error_str[len("resolver:"):])
    return PlaceholderError(error_str)


__all__ = [
    "PlaceholderClient",
    "PlaceholderCycleError",
    "PlaceholderDepthError",
    "PlaceholderError",
    "PlaceholderResolverError",
    "RenderResult",
    "resolve_uds_path",
]
