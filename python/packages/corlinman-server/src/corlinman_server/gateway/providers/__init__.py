"""``corlinman_server.gateway.providers`` — Parcel P1 provider-registry wiring.

This is the Wave-1 **P1** sibling module. It plugs into the
``entrypoint.py`` lifespan seam documented in
``docs/contracts/runtime-wiring.md`` §2: the lifespan iterates a fixed
list of sibling dotted names (``providers`` is first, before
``services``) and calls each module's optional ``bootstrap(state)``.

What :func:`bootstrap` does
---------------------------

1. Reads ``state.config["providers"]`` (a dict ``{name: {kind, api_key,
   base_url, enabled, params}}``) — the env-resolved runtime config the
   P0 loader attached to ``AppState.config``.
2. Builds a :class:`corlinman_providers.specs.ProviderSpec` per entry
   and constructs a :class:`corlinman_providers.registry.ProviderRegistry`
   from the validated specs.
3. Attaches the registry to ``state.provider_registry``.
4. Also attaches a :class:`RegistryModelSource` view to
   ``state.extras["models_source"]`` so the orchestrator can feed it to
   the ``/v1/models`` route (see this module's docstring tail + the
   integration note in the P1 report).

It follows the contract's **"gate, never crash"** rule: a bad provider
entry, an unknown ``kind``, or a missing ``[providers]`` section logs a
warning and leaves the rest of the registry intact. If construction
fails wholesale, ``state.provider_registry`` is left ``None`` and
``/v1/models`` returns its typed 501 envelope.

Wiring ``/v1/models``
---------------------

``routes/models.py`` exposes ``router(source: ModelSource | None)`` and
``routes/register.py``'s :class:`GatewayState` has a ``models_source``
field. ``bootstrap`` cannot reach those — they are constructed in
``entrypoint._mount_routes`` **before** the lifespan runs. So this
module does two things instead:

* attaches the live :class:`RegistryModelSource` to
  ``state.provider_registry`` (first-class field) and
  ``state.extras["models_source"]`` (handoff slot), and
* exposes :func:`model_source_for` so a route built lazily — i.e. one
  that reads ``AppState`` at request time — can construct the source on
  demand.

The orchestrator finishes the wiring with a small edit to
``register.py`` so ``/v1/models`` resolves its source from the live
``AppState`` per request. See the P1 report for the exact diff.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from typing import Any

import structlog

logger = structlog.get_logger(__name__)

__all__ = [
    "RegistryModelSource",
    "bootstrap",
    "build_registry",
    "model_source_for",
]


# ---------------------------------------------------------------------------
# Spec construction from the resolved config dict
# ---------------------------------------------------------------------------


def _build_specs(providers_cfg: Mapping[str, Any]) -> list[Any]:
    """Turn ``config["providers"]`` into a list of validated ``ProviderSpec``.

    ``providers_cfg`` is the env-resolved dict the P0 loader produced:
    ``{name: {kind, api_key, base_url, enabled, params}}``. Each entry is
    validated independently — a malformed entry (unknown ``kind``,
    missing required field) is logged and skipped so one bad stanza
    never bricks the whole provider plane. This mirrors the contract's
    "gate, never crash" rule.
    """
    from corlinman_providers.specs import ProviderSpec

    specs: list[Any] = []
    for name, raw in providers_cfg.items():
        if not isinstance(raw, Mapping):
            logger.warning(
                "gateway.providers.spec_not_a_table",
                provider=name,
                detail="[providers.<name>] entry is not a table; skipped",
            )
            continue
        # ``name`` is the table key; ``ProviderSpec`` wants it as a field.
        # A ``name`` inside the table (operator typo) is ignored in favour
        # of the key so the registry dict stays keyed consistently.
        data = {k: v for k, v in raw.items() if k != "name"}
        data["name"] = name
        try:
            specs.append(ProviderSpec.model_validate(data))
        except Exception as exc:  # noqa: BLE001 — pydantic ValidationError + co.
            logger.warning(
                "gateway.providers.spec_invalid",
                provider=name,
                kind=raw.get("kind"),
                error=str(exc),
                detail="provider entry failed validation; skipped",
            )
    return specs


def build_registry(
    config: Mapping[str, Any] | None,
    *,
    data_dir: Any = None,
) -> Any:
    """Build a :class:`ProviderRegistry` from a resolved gateway config.

    ``config`` is the dict on ``AppState.config`` (env-refs resolved).
    Reads ``config["providers"]``; an absent / empty section yields a
    specs-less registry (every ``resolve()`` then falls through to the
    legacy ``MODEL_PREFIX_DEFAULTS`` prefix table — still functional for
    bare ``ANTHROPIC_API_KEY`` / ``OPENAI_API_KEY`` env deployments).

    ``data_dir`` is forwarded to ``ProviderRegistry`` so OAuth-aware
    adapters can resolve their token file. Pass ``state.data_dir`` (the
    entrypoint stamps it onto ``AppState``).

    Raises nothing for per-spec problems (those are logged + skipped);
    only an import failure of ``corlinman_providers`` propagates, and
    :func:`bootstrap` catches that.
    """
    from corlinman_providers.registry import ProviderRegistry

    cfg = config or {}
    providers_cfg = cfg.get("providers") or {}
    if not isinstance(providers_cfg, Mapping):
        logger.warning(
            "gateway.providers.section_malformed",
            detail="[providers] is not a table; building empty registry",
        )
        providers_cfg = {}

    specs = _build_specs(providers_cfg)
    registry = ProviderRegistry(specs, data_dir=data_dir)
    logger.info(
        "gateway.providers.registry_built",
        configured=len(specs),
        built=len(registry.list_specs()),
    )
    return registry


# ---------------------------------------------------------------------------
# /v1/models model source
# ---------------------------------------------------------------------------


def _alias_entries(config: Mapping[str, Any] | None) -> list[tuple[str, str]]:
    """Extract ``(alias, owned_by)`` rows from ``config["models"]``.

    ``owned_by`` is the alias's target provider slot name — the closest
    OpenAI-``Model``-compatible "owner" we can surface. ``[models]`` may
    carry ``default`` plus an ``aliases`` table; only ``aliases`` rows
    become model ids.
    """
    cfg = config or {}
    models_cfg = cfg.get("models") or {}
    if not isinstance(models_cfg, Mapping):
        return []
    aliases = models_cfg.get("aliases") or {}
    if not isinstance(aliases, Mapping):
        return []
    rows: list[tuple[str, str]] = []
    for alias, entry in aliases.items():
        owner = "corlinman"
        if isinstance(entry, Mapping):
            owner = str(entry.get("provider") or owner)
        rows.append((str(alias), owner))
    return rows


class RegistryModelSource:
    """A ``routes.models.ModelSource`` view over a built provider plane.

    Implements ``list_models() -> Iterable[ModelEntry]`` (the structural
    protocol the ``/v1/models`` route depends on). The catalogue is the
    union of:

    * every ``[models.aliases]`` key — the user-facing model ids a chat
      client should ask for; and
    * every enabled ``[providers.<name>]`` slot name — surfaced so an
      operator probing ``/v1/models`` can see which provider planes are
      live even before aliases are configured.

    Built lazily / cheaply so it is safe to construct per request from
    the live ``AppState``.
    """

    __slots__ = ("_registry", "_config")

    def __init__(self, registry: Any, config: Mapping[str, Any] | None) -> None:
        self._registry = registry
        self._config = config or {}

    def list_models(self) -> Iterable[Any]:
        """Enumerate model ids known to the configured provider plane.

        Returns ``routes.models.ModelEntry`` instances. Imported lazily
        so this module stays importable even if the routes package is
        mid-port. De-duplicates while preserving first-seen order
        (aliases first, then provider slot names).
        """
        from corlinman_server.gateway.routes.models import ModelEntry

        seen: set[str] = set()
        entries: list[Any] = []

        for alias, owner in _alias_entries(self._config):
            if alias in seen:
                continue
            seen.add(alias)
            entries.append(ModelEntry(id=alias, owned_by=owner))

        # Provider slot names — useful when no aliases are declared. Only
        # specs that actually built (``get(name) is not None``) are
        # listed so a disabled / failed provider doesn't masquerade as a
        # usable model.
        registry = self._registry
        if registry is not None:
            try:
                specs = registry.list_specs()
            except Exception:  # noqa: BLE001 — defensive; never crash the route
                specs = []
            for spec in specs:
                name = getattr(spec, "name", None)
                if not name or name in seen:
                    continue
                if registry.get(name) is None:
                    continue
                seen.add(name)
                entries.append(ModelEntry(id=name, owned_by=name))

        return entries


def model_source_for(state: Any) -> RegistryModelSource | None:
    """Return a :class:`RegistryModelSource` for ``state`` or ``None``.

    ``None`` when no registry is wired (``state.provider_registry`` is
    ``None``) — the ``/v1/models`` route then keeps its typed 501
    envelope. Safe to call per request; construction is cheap.
    """
    registry = getattr(state, "provider_registry", None)
    if registry is None:
        return None
    return RegistryModelSource(registry, getattr(state, "config", None))


# ---------------------------------------------------------------------------
# The sibling bootstrap seam
# ---------------------------------------------------------------------------


def bootstrap(state: Any) -> None:
    """Startup wiring — build the provider registry, attach it to ``state``.

    Called once by the gateway lifespan (``entrypoint.py``) per the
    ``docs/contracts/runtime-wiring.md`` §2 seam, before the gateway
    accepts requests and before the ``services`` sibling boots (so the
    chat/channel bootstraps see a populated ``provider_registry``).

    Sets, on ``state``:

    * ``state.provider_registry`` — a
      :class:`corlinman_providers.registry.ProviderRegistry` built from
      ``state.config["providers"]``.
    * ``state.extras["models_source"]`` — a :class:`RegistryModelSource`
      ready for the ``/v1/models`` route.

    Returns ``None`` (no background tasks). On any failure the
    ``provider_registry`` slot is left ``None`` and ``/v1/models``
    returns 501 ``no ProviderRegistry wired`` — degraded, not crashed.
    """
    config = getattr(state, "config", None)
    data_dir = getattr(state, "data_dir", None)
    try:
        registry = build_registry(config, data_dir=data_dir)
    except Exception as exc:  # noqa: BLE001 — gate, never crash the boot.
        logger.warning(
            "gateway.providers.bootstrap_failed",
            error=str(exc),
            detail="provider registry not built; /v1/models stays degraded",
        )
        return None

    state.provider_registry = registry
    source = RegistryModelSource(registry, config)
    # ``extras`` is the documented free-form handoff bag. The registry
    # itself is the load-bearing handle (first-class field above); the
    # model source is a derived view, so it lives in ``extras`` for the
    # orchestrator / route to pick up.
    extras = getattr(state, "extras", None)
    if isinstance(extras, dict):
        extras["models_source"] = source

    logger.info(
        "gateway.providers.bootstrap_done",
        models=len(list(source.list_models())),
    )
    return None
