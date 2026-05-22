"""``grpc_backend`` — Parcel **P4** chat-backend wiring (full agent path).

Parcel **P4** of the Python-port runtime-completion plan
(``docs/PLAN_PORT_COMPLETION.md`` §3, Wave 1). See
``docs/contracts/runtime-wiring.md`` §4 for the ``ChatBackend`` contract
and §2 for the sibling-``bootstrap`` seam this module feeds.

What P4 is — and what it is *not*
---------------------------------

The plan framed P4 as "stand up a real gRPC ``Agent`` server, replacing
``_NullEngine`` / the placeholder". The feasibility pass found that
framing inaccurate:

* ``_NullEngine`` (in ``gateway/grpc/placeholder.py``) belongs to the
  **Placeholder** service — ``{{token}}`` rendering — *not* the Agent
  service. It is not a stand-in for the agent.
* A **real, complete** ``corlinman.v1.Agent`` gRPC server already
  exists: :class:`corlinman_server.agent_servicer.CorlinmanAgentServicer`
  drives :class:`corlinman_agent.reasoning_loop.ReasoningLoop` with the
  full tool / subagent / context-assembler surface, and
  :func:`corlinman_server.main.main` (the ``corlinman-python-server``
  console script) serves it over a UDS / TCP. Both are covered by
  ``tests/test_agent_servicer.py`` + ``tests/test_tool_loop_closes.py``.

So P4 is **not** a "build the agent" task — the agent is fully ported.
P4 is the **gateway-side wiring**: connect the (already-implemented)
:class:`~corlinman_server.gateway.services.chat_service.GrpcAgentChatBackend`
to a running agent and surface it as a :class:`ChatService` on
``AppState.chat``.

Two backends, one deployment switch
-----------------------------------

Both backends implement the ``ChatBackend`` protocol:

* ``DirectProviderBackend`` (P2) — calls :mod:`corlinman_providers`
  straight. No tools, no agent, no memory. The fast path.
* ``GrpcAgentChatBackend`` (already in ``chat_service.py``) — dials the
  full Python agent over ``grpc.aio``. Tools, subagents, context
  assembly, session memory. The complete path.

The gateway picks one per deployment via :func:`chat_backend_mode`,
which reads ``config["models"]["backend"]`` (``"direct"`` —
the default — or ``"grpc_agent"``). :func:`build_chat_service` is the
single entry point the ``services`` sibling ``bootstrap`` calls to get
whichever ``ChatService`` the operator asked for.

Endpoint resolution
-------------------

When ``grpc_agent`` mode is selected, the agent endpoint is resolved by
:func:`corlinman_grpc.agent_client.resolve_endpoint` —
``CORLINMAN_PY_ADDR`` > ``CORLINMAN_PY_PORT`` > ``127.0.0.1:50051``, plus
this module's ``CORLINMAN_PY_SOCKET`` UDS support (matching
``corlinman_server.main._bind_address`` so the gateway dials the same
place the ``corlinman-python-server`` process binds). The channel is
lazy — building the backend never blocks on a handshake; a dead agent
surfaces as a terminal ``error`` frame on the first chat turn, not a
boot crash.
"""

from __future__ import annotations

import logging
import os
from typing import Any

__all__ = [
    "DEFAULT_AGENT_ENDPOINT",
    "build_chat_service",
    "build_grpc_chat_service",
    "chat_backend_mode",
    "resolve_agent_target",
]

log = logging.getLogger(__name__)

#: Fallback gRPC target when no env override is set. Mirrors
#: :data:`corlinman_grpc.agent_client.DEFAULT_TCP_ADDR` and
#: :data:`corlinman_server.main._DEFAULT_TCP_ADDR`.
DEFAULT_AGENT_ENDPOINT: str = "127.0.0.1:50051"

#: Recognised values for ``config["models"]["backend"]``.
_MODE_DIRECT = "direct"
_MODE_GRPC = "grpc_agent"
_VALID_MODES = frozenset({_MODE_DIRECT, _MODE_GRPC})


# ---------------------------------------------------------------------------
# Deployment-mode selection
# ---------------------------------------------------------------------------


def chat_backend_mode(state: Any) -> str:
    """Resolve which chat backend the deployment wants.

    Precedence:

    1. ``$CORLINMAN_CHAT_BACKEND`` env var (``direct`` / ``grpc_agent``)
       — an operator escape hatch that works without editing the TOML.
    2. ``config["models"]["backend"]`` — the declarative config knob.
    3. Default: ``"direct"`` (P2's fast path) — the gRPC agent server is
       a separate process, so a deployment that didn't opt in keeps the
       in-process direct path that needs no extra wiring.

    An unrecognised value logs a warning and falls back to ``"direct"``.
    Returns one of :data:`_MODE_DIRECT` / :data:`_MODE_GRPC`.
    """
    env = (os.environ.get("CORLINMAN_CHAT_BACKEND") or "").strip().lower()
    if env:
        if env in _VALID_MODES:
            return env
        log.warning(
            "grpc_backend.unknown_mode_env value=%s; falling back to %s",
            env,
            _MODE_DIRECT,
        )
        return _MODE_DIRECT

    cfg = getattr(state, "config", None) or {}
    models_cfg = cfg.get("models") if isinstance(cfg, dict) else None
    if isinstance(models_cfg, dict):
        raw = models_cfg.get("backend")
        if raw is not None:
            mode = str(raw).strip().lower()
            if mode in _VALID_MODES:
                return mode
            log.warning(
                "grpc_backend.unknown_mode_config value=%s; falling back to %s",
                raw,
                _MODE_DIRECT,
            )
    return _MODE_DIRECT


# ---------------------------------------------------------------------------
# Endpoint resolution
# ---------------------------------------------------------------------------


def resolve_agent_target(state: Any | None = None) -> str:
    """Resolve the ``grpc.aio`` target for the Python agent server.

    Precedence (mirrors ``corlinman_server.main._bind_address`` so the
    gateway dials exactly where ``corlinman-python-server`` binds):

    1. ``config["agent"]["endpoint"]`` — explicit declarative override.
    2. ``$CORLINMAN_PY_SOCKET`` — Unix domain socket path → ``unix://``.
    3. ``$CORLINMAN_PY_ADDR`` — explicit ``host:port``.
    4. ``$CORLINMAN_PY_PORT`` — port only, bound to ``127.0.0.1``.
    5. :data:`DEFAULT_AGENT_ENDPOINT`.

    ``connect_channel`` in :mod:`corlinman_grpc.agent_client` strips a
    leading ``http(s)://`` and accepts ``unix:`` targets, so the string
    returned here is handed straight to it.
    """
    # 1 — declarative config override.
    if state is not None:
        cfg = getattr(state, "config", None) or {}
        agent_cfg = cfg.get("agent") if isinstance(cfg, dict) else None
        if isinstance(agent_cfg, dict):
            endpoint = agent_cfg.get("endpoint")
            if endpoint:
                return str(endpoint)

    # 2-4 — env overrides, matching ``main._bind_address`` shape.
    sock = os.environ.get("CORLINMAN_PY_SOCKET")
    if sock:
        # ``grpc.aio`` UDS URIs use the single-slash ``unix:`` form;
        # ``connect_channel`` passes the target through verbatim.
        return f"unix:{sock}"
    addr = os.environ.get("CORLINMAN_PY_ADDR")
    if addr:
        return addr
    port = os.environ.get("CORLINMAN_PY_PORT")
    if port:
        return f"127.0.0.1:{port}"

    # 5 — default.
    return DEFAULT_AGENT_ENDPOINT


# ---------------------------------------------------------------------------
# ChatService builders
# ---------------------------------------------------------------------------


def build_grpc_chat_service(state: Any) -> Any | None:
    """Build a :class:`ChatService` over a :class:`GrpcAgentChatBackend`.

    Opens a lazily-connecting :class:`grpc.aio.Channel` to the Python
    agent (target from :func:`resolve_agent_target`), wraps it in an
    :class:`~corlinman_grpc.agent_client.AgentClient`, and hands that to
    :class:`~corlinman_server.gateway.services.chat_service.GrpcAgentChatBackend`.

    Returns the built :class:`ChatService`, or ``None`` on any wiring
    failure (degraded mode — ``/v1/chat/completions`` keeps its typed
    501). Follows the contract's "gate, never crash" rule: a missing
    sibling package or a malformed endpoint logs a warning and degrades;
    it never raises into the gateway boot.

    The channel is **lazy** — construction does not block on a TCP / UDS
    handshake. If the ``corlinman-python-server`` agent process is not
    running, the failure surfaces as a terminal ``error`` frame on the
    first chat turn (``GrpcAgentChatBackend.start`` → the bidi call
    fails), which the :class:`ChatService` renders as a clean upstream
    error — exactly the behaviour the contract's degraded story wants.
    """
    try:
        from corlinman_grpc.agent_client import AgentClient, connect_channel

        from corlinman_server.gateway.services.chat_service import (
            ChatService,
            GrpcAgentChatBackend,
        )
    except Exception as exc:
        log.warning("grpc_backend.import_failed err=%s", exc)
        return None

    target = resolve_agent_target(state)
    try:
        channel = connect_channel(target)
    except Exception as exc:
        log.warning(
            "grpc_backend.connect_channel_failed target=%s err=%s",
            target,
            exc,
        )
        return None

    client = AgentClient(channel)
    backend = GrpcAgentChatBackend(client)
    log.info(
        "grpc_backend.chat_service_built backend=GrpcAgentChatBackend "
        "target=%s",
        target,
    )
    return ChatService(backend)


def build_chat_service(state: Any) -> Any | None:
    """Build the chat service the deployment asked for.

    Single entry point for the ``services`` sibling ``bootstrap``:

    * ``grpc_agent`` mode → :func:`build_grpc_chat_service` (full agent
      path — tools / subagents / memory).
    * ``direct`` mode (default) → delegates to P2's
      :func:`corlinman_server.gateway.services.chat_bootstrap.build_chat_service`
      when that module is present; otherwise returns ``None``.

    Returns the built :class:`ChatService`, or ``None`` for degraded
    mode. Never raises.

    Integration note
    ----------------

    The ``services`` package exposes exactly one ``bootstrap`` symbol
    (the entrypoint seam does ``getattr(services_module, "bootstrap")``).
    P2 owns ``services/chat_bootstrap.py``; the orchestrator wires the
    package ``bootstrap`` to call **this** function so a single hook
    serves both modes. See this module's docstring + the P4 report for
    the exact ``services/__init__.py`` edit.
    """
    mode = chat_backend_mode(state)
    if mode == _MODE_GRPC:
        return build_grpc_chat_service(state)

    # Direct mode — delegate to P2's builder if it has landed.
    try:
        from corlinman_server.gateway.services.chat_bootstrap import (
            build_chat_service as _p2_build,
        )
    except Exception as exc:
        log.warning(
            "grpc_backend.direct_builder_unavailable err=%s; "
            "chat service not wired",
            exc,
        )
        return None
    return _p2_build(state)
