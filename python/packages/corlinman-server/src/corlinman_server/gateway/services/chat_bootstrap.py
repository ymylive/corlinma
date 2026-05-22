"""``corlinman_server.gateway.services`` sibling-``bootstrap`` seam.

Parcel **P2** of the Python-port runtime-completion plan
(``docs/PLAN_PORT_COMPLETION.md`` §3, Wave 1). See
``docs/contracts/runtime-wiring.md`` §2 for the seam contract.

The gateway lifespan (``gateway/lifecycle/entrypoint.py``) iterates a
fixed list of sibling modules and, for each, calls an optional
``bootstrap(state)``. ``corlinman_server.gateway.services`` is one of
those siblings; this module is the body of its ``bootstrap``. It is
re-exported from ``gateway/services/__init__.py`` as the package's
``bootstrap`` symbol so the entrypoint seam's
``getattr(services_module, "bootstrap")`` resolves to it. (The module
is named ``chat_bootstrap`` rather than ``bootstrap`` so the dotted
submodule name does not collide with that re-exported function.)

``bootstrap(state)`` here is the **P2 chat-service** wiring: it builds a
:class:`~corlinman_server.gateway.services.chat_service.ChatService`
around a :class:`~corlinman_server.gateway.services.direct_backend.DirectProviderBackend`
and attaches it to ``state.chat``. P3 (channel runtime) extends this
seam — see :func:`bootstrap` for the documented extension point.

Why the direct backend?
-----------------------

Two backends implement the ``ChatBackend`` protocol:

* :class:`DirectProviderBackend` (P2) — calls :mod:`corlinman_providers`
  straight, no agent, no tools. Fast path; this is what
  :func:`bootstrap` wires by default.
* ``GrpcAgentChatBackend`` (P4) — dials the full Python agent over
  gRPC (tools / skills / memory).

The gateway picks one per deployment. Until P4's gRPC agent server is
running, the direct backend is the only one that yields a real
completion, so :func:`bootstrap` selects it whenever
``state.provider_registry`` is populated.
"""

from __future__ import annotations

import logging
from typing import Any

__all__ = [
    "bootstrap",
    "build_chat_service",
]

log = logging.getLogger(__name__)


def build_chat_service(state: Any) -> Any | None:
    """Build a :class:`ChatService` over a :class:`DirectProviderBackend`.

    Reads ``state.provider_registry`` (the handle P1 attaches) and
    ``state.config["models"]`` (for alias resolution). Returns the built
    :class:`ChatService`, or ``None`` when no provider registry is wired
    (degraded mode — ``/v1/chat/completions`` then keeps its 501).

    Split out from :func:`bootstrap` so tests (and P3's channel wiring)
    can build the service without going through the full sibling seam.
    """
    registry = getattr(state, "provider_registry", None)
    if registry is None:
        log.warning(
            "services.bootstrap.no_provider_registry; "
            "ChatService not wired, /v1/chat/completions stays degraded",
        )
        return None

    # Lazy imports — keep this module importable even if a sibling
    # package is mid-port. A failure here logs + degrades, never crashes
    # the gateway boot (contract §2.1 "gate, never crash").
    try:
        from corlinman_server.gateway.services.chat_service import ChatService
        from corlinman_server.gateway.services.direct_backend import (
            DirectProviderBackend,
        )
    except Exception as exc:  # noqa: BLE001 — degrade, don't crash boot
        log.warning("services.bootstrap.import_failed err=%s", exc)
        return None

    cfg = getattr(state, "config", None) or {}
    models_config = cfg.get("models") if isinstance(cfg, dict) else None
    if not isinstance(models_config, dict):
        models_config = {}

    backend = DirectProviderBackend(registry, models_config=models_config)
    return ChatService(backend)


def bootstrap(state: Any) -> None:
    """Sibling ``bootstrap`` hook — attach the chat service to ``state``.

    Called once during the gateway lifespan (``entrypoint.py``), *after*
    the ``providers`` sibling has populated ``state.provider_registry``
    (the seam order is load-bearing — see contract §2). Mutates ``state``
    in place:

    * ``state.chat`` ← a :class:`ChatService` wrapping a
      :class:`DirectProviderBackend`, or left ``None`` when no provider
      registry is available (degraded mode).

    Returns ``None`` — the chat service owns no background tasks. (P3's
    channel runtime, layered into this same seam, *does* return
    ``asyncio.Task``s; when P3 lands it should call :func:`build_chat_service`
    / reuse ``state.chat`` here and return its channel tasks. The
    entrypoint already accepts ``None`` | ``Awaitable`` | ``list[Task]``,
    so extending the return type is forward-compatible.)
    """
    if getattr(state, "chat", None) is not None:
        # Idempotent: another wiring path (a test, a P4 deployment that
        # pre-built a gRPC-backed service) already populated it. Don't
        # clobber it with the direct backend.
        log.info("services.bootstrap.chat_already_wired; skipping")
        return None

    service = build_chat_service(state)
    if service is None:
        return None

    state.chat = service
    log.info("services.bootstrap.chat_wired backend=DirectProviderBackend")
    return None
