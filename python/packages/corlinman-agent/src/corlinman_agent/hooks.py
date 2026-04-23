"""Hook-bus emitter stub — Python-side placeholder for the future gRPC
hook service.

The hook bus is currently Rust-only. While B2-BE4 is landing, we need a
side-effect-free way for the context assembler to announce lifecycle
events (``message.preprocessed``, eventually ``skill.injected``, etc.)
without coupling it to a specific transport. This module exposes a
narrow :class:`HookEmitter` protocol plus a :class:`LoggingHookEmitter`
default that just writes structured logs.

Future work
-----------

Once the gateway exposes a ``Hook`` gRPC service over the same UDS the
placeholder client uses, a new ``GrpcHookEmitter`` will land here with
the same :meth:`emit` contract. Callers that bind against the Protocol
rather than :class:`LoggingHookEmitter` will pick up the upgrade
automatically.
"""

from __future__ import annotations

from typing import Any, Protocol

import structlog

logger = structlog.get_logger(__name__)


class HookEmitter(Protocol):
    """Minimal emitter contract used by the context assembler.

    ``emit`` is synchronous on purpose: every known consumer today
    either enqueues on an in-memory list (tests), writes a structured
    log line, or fires-and-forgets a non-blocking gRPC request from a
    background task. Making the method async would force every caller
    into an ``await`` point that buys nothing at the call site.
    """

    def emit(self, kind: str, payload: dict[str, Any]) -> None:  # pragma: no cover — protocol
        ...


class LoggingHookEmitter:
    """Default :class:`HookEmitter` that writes a structured log line.

    Useful in dev, CI, and as a fallback when no gRPC bus is reachable.
    Logs at ``info`` level so operators can filter on ``hook_kind``
    without needing ``debug`` plumbing turned on everywhere.
    """

    def emit(self, kind: str, payload: dict[str, Any]) -> None:
        logger.info("hook", hook_kind=kind, payload=payload)


class RecordingHookEmitter:
    """Test helper — records every emission on an in-memory list.

    Lives in the production module (not the tests folder) because the
    reasoning loop's own tests and the context assembler's goldens both
    need to capture emissions without pulling in a fixture.
    """

    def __init__(self) -> None:
        self.events: list[tuple[str, dict[str, Any]]] = []

    def emit(self, kind: str, payload: dict[str, Any]) -> None:
        # Copy the payload so later mutation on the caller's side can't
        # retroactively rewrite what tests see.
        self.events.append((kind, dict(payload)))


__all__ = ["HookEmitter", "LoggingHookEmitter", "RecordingHookEmitter"]
