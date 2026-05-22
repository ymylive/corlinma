"""``corlinman_server.gateway.channels_runtime`` — channel bootstrap.

Parcel **P3** of the Python-port runtime-completion plan
(``docs/PLAN_PORT_COMPLETION.md`` §3, contract in
``docs/contracts/runtime-wiring.md``).

The Rust gateway spawned every enabled inbound channel adapter at boot.
The Python port had ``corlinman-channels`` fully ported
(``run_qq_channel`` / ``run_telegram_channel`` + the OneBot / Telegram
adapters) but **zero callers** — the gateway booted with no channels and
no bot ever connected to NapCat / Telegram. This module is the missing
caller.

What it does
------------

:func:`bootstrap` is a sibling ``bootstrap(state)`` hook (see contract
§2). It reads ``state.config["channels"]``, and for each *enabled*
channel (``qq`` / ``telegram``) builds the channel-params object from
``corlinman-channels`` and launches ``run_qq_channel`` /
``run_telegram_channel`` as a background :class:`asyncio.Task`. The list
of tasks is returned so the gateway lifespan registers them into its
``background`` list and cancels + awaits them at shutdown under the
shared ``cancel`` event.

Cancellation contract
---------------------

``run_qq_channel`` / ``run_telegram_channel`` each take an
``asyncio.Event`` and exit cleanly once it is set. P3 owns a single
``asyncio.Event`` shared by every channel task; it is *not* the
lifespan's own ``cancel`` event (the seam does not pass that event to
``bootstrap``) — instead :func:`bootstrap` wraps each channel coroutine
so that when the **task is cancelled** by the lifespan-exit ``finally``
the event is set, giving the adapter a graceful-shutdown window before
the ``CancelledError`` propagates. This keeps the contract "returned
tasks are cancelled + awaited at shutdown" working without needing the
lifespan to hand us its event.

Degraded-mode behaviour
-----------------------

Every failure mode is a gate, never a crash (contract §2.2):

* no ``[channels]`` section / no enabled channel → returns ``[]``.
* ``state.chat`` is ``None`` (P2 not wired / degraded) → the channels
  still start, but ``run_*_channel`` drops inbound messages silently
  because ``chat_service is None``. We log a warning so the operator
  sees *why* the bot is silent.
* a per-channel build error (missing ``ws_url`` / ``bot_token``) is
  logged and that channel is skipped; the others still start.
"""

from __future__ import annotations

import asyncio
import os
from collections.abc import Mapping
from typing import Any

import structlog

logger = structlog.get_logger(__name__)

__all__ = ["bootstrap", "build_channel_tasks"]


# ---------------------------------------------------------------------------
# Config helpers
# ---------------------------------------------------------------------------


def _as_mapping(value: Any) -> Mapping[str, Any]:
    """Return ``value`` if it is a mapping, else an empty dict.

    ``AppState.config`` is the plain env-resolved dict from
    ``gateway.core.config.load_from_path`` (contract §1.1) — every
    section is read with ``.get``. A missing / mistyped section
    collapses to ``{}`` so callers never ``AttributeError``.
    """
    return value if isinstance(value, Mapping) else {}


def _is_enabled(section: Mapping[str, Any]) -> bool:
    """A channel is enabled iff its ``enabled`` key is truthy.

    Mirrors the Rust gate (``cfg.channels.qq.enabled``). The key
    defaults to ``False`` when absent so a half-written ``[channels.qq]``
    stub never auto-starts a bot.
    """
    return bool(section.get("enabled", False))


# ---------------------------------------------------------------------------
# Per-channel param builders
# ---------------------------------------------------------------------------


def _build_qq_params(qq_cfg: Mapping[str, Any], model: str, chat_service: Any) -> Any:
    """Build :class:`corlinman_channels.QqChannelParams` from the
    ``[channels.qq]`` config table.

    The ``ws_url`` may be supplied either inline in the TOML or — the
    common docker-compose path — via the ``QQ_WS_URL`` env var that
    ``docker-compose.qq.yml`` injects (the config template documents
    exactly this). We resolve the env var as a fallback so the standard
    NapCat deployment works without the operator hand-editing the TOML.
    """
    from corlinman_channels import QqChannelParams

    # ``ws_url`` precedence: explicit config value → ``QQ_WS_URL`` env →
    # empty (``run_qq_channel`` then raises a clear ValueError, which the
    # caller logs + skips).
    ws_url = qq_cfg.get("ws_url") or os.environ.get("QQ_WS_URL") or ""

    # ``run_qq_channel`` reads ``config`` structurally via ``_attr`` which
    # already tolerates a dict; we hand it a plain dict with the env-ref
    # already resolved (config loader did that) and ws_url backfilled.
    cfg: dict[str, Any] = dict(qq_cfg)
    if ws_url:
        cfg["ws_url"] = ws_url

    return QqChannelParams(
        config=cfg,
        model=model,
        chat_service=chat_service,
    )


def _build_telegram_params(
    tg_cfg: Mapping[str, Any], model: str, chat_service: Any
) -> Any:
    """Build :class:`corlinman_channels.TelegramChannelParams` from the
    ``[channels.telegram]`` config table.

    ``run_telegram_channel`` reads ``bot_token`` / ``allowed_chat_ids`` /
    ``keyword_filter`` / ``require_mention_in_groups`` / ``base_url`` off
    the structural ``config``; the dict from the loader satisfies that.
    """
    from corlinman_channels import TelegramChannelParams

    return TelegramChannelParams(
        config=dict(tg_cfg),
        model=model,
        chat_service=chat_service,
    )


# ---------------------------------------------------------------------------
# Task wrapper — bridge task-cancellation to the channel cancel Event
# ---------------------------------------------------------------------------


async def _run_channel(
    name: str,
    coro_factory: Any,
    cancel: asyncio.Event,
) -> None:
    """Run one ``run_*_channel`` coroutine, translating an outer
    task-cancel into a clean ``cancel.set()``.

    ``run_qq_channel`` / ``run_telegram_channel`` loop until ``cancel``
    is set. The gateway lifespan shuts a background task down with
    ``task.cancel()`` (it does not hand ``bootstrap`` its ``cancel``
    event). Wrapping the coroutine here lets us intercept that
    ``CancelledError``, set our own ``cancel`` event so the adapter can
    close its WebSocket / HTTP client gracefully, then await the
    coroutine's own clean exit before re-raising.
    """
    runner = asyncio.ensure_future(coro_factory(cancel))
    try:
        await asyncio.shield(runner)
    except asyncio.CancelledError:
        # Lifespan-exit cancel — give the adapter a graceful window.
        logger.info("gateway.channels.shutdown", channel=name)
        cancel.set()
        try:
            await runner
        except asyncio.CancelledError:  # pragma: no cover — defensive
            pass
        except Exception as exc:  # pragma: no cover — adapter-owned
            logger.warning(
                "gateway.channels.shutdown_error", channel=name, error=str(exc)
            )
        raise
    except Exception as exc:
        # The channel loop crashed on its own (lost WS, bad token mid-run).
        # Log it; the task ends. The gateway stays up — a dead channel is
        # not a dead gateway.
        logger.warning(
            "gateway.channels.channel_crashed", channel=name, error=str(exc)
        )
    else:
        logger.info("gateway.channels.channel_exited", channel=name)


# ---------------------------------------------------------------------------
# Task assembly (split out so tests can drive it without a full AppState)
# ---------------------------------------------------------------------------


def build_channel_tasks(
    channels_cfg: Mapping[str, Any],
    *,
    model: str,
    chat_service: Any,
    cancel: asyncio.Event,
) -> list[asyncio.Task[Any]]:
    """Build (but the caller owns scheduling) the channel background
    tasks for every enabled channel in ``channels_cfg``.

    Pure-ish factory: it does call :func:`asyncio.create_task`, so it
    must run inside a running loop, but it takes plain values rather
    than an :class:`AppState` so unit tests can exercise it directly.

    A per-channel build failure is logged and skipped — the surviving
    channels still get their task. Returns the list of created tasks
    (possibly empty).
    """
    tasks: list[asyncio.Task[Any]] = []

    # --- QQ / OneBot --------------------------------------------------------
    qq_cfg = _as_mapping(channels_cfg.get("qq"))
    if _is_enabled(qq_cfg):
        try:
            from corlinman_channels import run_qq_channel

            params = _build_qq_params(qq_cfg, model, chat_service)
            task = asyncio.create_task(
                _run_channel(
                    "qq",
                    lambda c, p=params: run_qq_channel(p, c),
                    cancel,
                ),
                name="channel-qq",
            )
            tasks.append(task)
            logger.info(
                "gateway.channels.started",
                channel="qq",
                model=model,
                has_chat_service=chat_service is not None,
            )
        except Exception as exc:
            logger.warning(
                "gateway.channels.build_failed", channel="qq", error=str(exc)
            )
    elif qq_cfg:
        logger.debug("gateway.channels.disabled", channel="qq")

    # --- Telegram -----------------------------------------------------------
    tg_cfg = _as_mapping(channels_cfg.get("telegram"))
    if _is_enabled(tg_cfg):
        try:
            from corlinman_channels import run_telegram_channel

            params = _build_telegram_params(tg_cfg, model, chat_service)
            task = asyncio.create_task(
                _run_channel(
                    "telegram",
                    lambda c, p=params: run_telegram_channel(p, c),
                    cancel,
                ),
                name="channel-telegram",
            )
            tasks.append(task)
            logger.info(
                "gateway.channels.started",
                channel="telegram",
                model=model,
                has_chat_service=chat_service is not None,
            )
        except Exception as exc:
            logger.warning(
                "gateway.channels.build_failed",
                channel="telegram",
                error=str(exc),
            )
    elif tg_cfg:
        logger.debug("gateway.channels.disabled", channel="telegram")

    return tasks


# ---------------------------------------------------------------------------
# The sibling bootstrap hook
# ---------------------------------------------------------------------------


def bootstrap(state: Any) -> list[asyncio.Task[Any]]:
    """Sibling ``bootstrap(state)`` hook — start every enabled channel.

    Contract (``docs/contracts/runtime-wiring.md`` §2):

    * reads ``state.config["channels"]`` (env-resolved dict);
    * for each enabled channel builds the params with
      ``chat_service=state.chat`` and ``model`` from
      ``config["models"]["default"]``;
    * launches ``run_qq_channel`` / ``run_telegram_channel`` as
      background :class:`asyncio.Task`;
    * **returns the list of tasks** so the lifespan registers them into
      its ``background`` list and cancels + awaits them at shutdown.

    Must run inside the gateway lifespan (a running event loop) — it
    calls :func:`asyncio.create_task`. The contract's seam invokes
    ``bootstrap`` exactly there, so that holds.

    Degraded gates (never crashes the boot):

    * no config / no ``[channels]`` → ``[]``;
    * ``state.chat`` is ``None`` → channels still start but log a warning
      that inbound messages will be dropped (no backend wired);
    * a missing ``corlinman-channels`` import → logged, ``[]``.
    """
    cfg = _as_mapping(getattr(state, "config", None))
    channels_cfg = _as_mapping(cfg.get("channels"))
    if not channels_cfg:
        logger.debug("gateway.channels.no_config")
        return []

    models_cfg = _as_mapping(cfg.get("models"))
    model = str(models_cfg.get("default") or "")

    chat_service = getattr(state, "chat", None)
    if chat_service is None:
        # P2 not wired (degraded). The channels still connect to NapCat /
        # Telegram so ``/admin/channels/qq/status`` shows online, but
        # ``run_*_channel`` drops every inbound message because
        # ``chat_service is None``. Surface the reason loudly.
        logger.warning(
            "gateway.channels.no_chat_service",
            detail=(
                "channels will connect but cannot reply — "
                "AppState.chat is None (ChatService not wired by P2)"
            ),
        )

    # One cancel Event shared by every channel task. ``_run_channel``
    # sets it when a task is cancelled at lifespan exit, so all channels
    # drain together.
    cancel = asyncio.Event()

    try:
        tasks = build_channel_tasks(
            channels_cfg,
            model=model,
            chat_service=chat_service,
            cancel=cancel,
        )
    except Exception as exc:  # pragma: no cover — defensive umbrella
        logger.warning("gateway.channels.bootstrap_failed", error=str(exc))
        return []

    if not tasks:
        logger.info("gateway.channels.none_enabled")
    else:
        logger.info("gateway.channels.bootstrap_done", count=len(tasks))

    # Stash the shared cancel Event on the state so other components /
    # tests can introspect it. Best-effort — AppState has no __slots__.
    try:
        state.channels_cancel = cancel
    except (AttributeError, TypeError):  # pragma: no cover — defensive
        pass

    return tasks
