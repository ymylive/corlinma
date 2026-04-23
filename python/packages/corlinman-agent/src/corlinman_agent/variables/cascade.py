"""Variable cascade engine (fixed / tar / var / sar tiers).

Resolution priority, highest first:

1. **Fixed** — hardcoded Python resolvers (``TimeVar``, ``Date``, plus
   any user registration). Always wins.
2. **Tar** — key prefix ``Tar``; loads ``TVStxt/tar/<rest>.txt``.
3. **Var** — key prefix ``Var``; consults ``os.environ[key]``. If the
   env value matches a filename stem under ``TVStxt/var``, the file's
   contents are returned; otherwise the env value is the value.
4. **Sar** — model-gated. Keys match ``^Sar(?P<body>[A-Za-z]+)(?P<n>\\d+)$``
   (e.g. ``SarPrompt4``). Look up ``os.environ["SarModel<N>"]``; if the
   current model name appears in that comma-separated list
   (case-insensitive), load ``TVStxt/sar/<key>.txt``; else return
   ``""``.

``resolve()`` returns:

* the substitution string for a known key,
* ``""`` for Sar keys that are defined but whose model gate missed
  (so the placeholder engine drops the token rather than leaving
  ``{{SarPromptN}}`` raw),
* ``None`` for unknown keys so the placeholder engine can leave them
  untouched for another namespace to pick up.
"""

from __future__ import annotations

import os
import re
from collections.abc import Callable
from pathlib import Path

from corlinman_agent.variables.fixed import FixedRegistry
from corlinman_agent.variables.hot_reload import HotReloadWatcher
from corlinman_agent.variables.loaders import DirLoader

# Sar keys are ``Sar`` + one or more letters + an integer suffix.
# Examples: ``SarPrompt4``, ``SarSystem12``. The integer is what pairs
# the key with its ``SarModel<N>`` gate env var.
_SAR_KEY_RE = re.compile(r"^Sar[A-Za-z]+(?P<n>\d+)$")


class VariableCascade:
    """Synchronous variable resolver with async hot-reload."""

    def __init__(
        self,
        tar_dir: Path | None,
        var_dir: Path | None,
        sar_dir: Path | None,
        fixed_dir: Path | None,
        hot_reload: bool = True,
    ) -> None:
        self._tar = DirLoader(tar_dir)
        self._var = DirLoader(var_dir)
        self._sar = DirLoader(sar_dir)
        # ``fixed_dir`` is a hook for future user overrides stored on
        # disk; we accept it and expose the loader but the fixed
        # registry still takes precedence for now.
        self._fixed_dir = DirLoader(fixed_dir)
        self._fixed = FixedRegistry()
        self._hot_reload_enabled = hot_reload
        self._watcher: HotReloadWatcher | None = None

    # ------------------------------------------------------------------ API

    def resolve(self, key: str, model_name: str) -> str | None:
        """Resolve ``key`` against the 4-tier cascade.

        See module docstring for return-value semantics.
        """
        # 1. Fixed — user and built-in Python resolvers.
        if self._fixed.has(key):
            return self._fixed.resolve(key)

        # 2. Tar — prefix-based file lookup.
        if key.startswith("Tar") and len(key) > len("Tar"):
            stem = key[len("Tar"):]
            hit = self._tar.load(stem)
            if hit is not None:
                return hit
            # Tar key shaped correctly but no file → fall through so
            # Var can't accidentally pick it up. Unknown = None.
            return None

        # 3. Sar — checked before Var so ``SarPrompt4`` doesn't get
        # mis-routed through ``Var`` / env fallback.
        sar_match = _SAR_KEY_RE.match(key)
        if sar_match is not None:
            return self._resolve_sar(key, sar_match.group("n"), model_name)

        # 4. Var — env-backed with optional file pivot.
        if key.startswith("Var") and len(key) > len("Var"):
            return self._resolve_var(key)

        return None

    def register_fixed(self, key: str, resolver: Callable[[], str]) -> None:
        self._fixed.register(key, resolver)

    async def start_watching(self) -> None:
        if not self._hot_reload_enabled or self._watcher is not None:
            return
        self._watcher = HotReloadWatcher(
            [self._tar, self._var, self._sar, self._fixed_dir]
        )
        await self._watcher.start()

    async def stop_watching(self) -> None:
        w = self._watcher
        if w is None:
            return
        self._watcher = None
        await w.stop()

    # -------------------------------------------------------------- internals

    def _resolve_var(self, key: str) -> str | None:
        env_value = os.environ.get(key)
        if env_value is None:
            # Unknown var key: return None so the placeholder engine
            # can leave the token literal for another resolver.
            return None
        # File-pivot: env value names a stem under TVStxt/var.
        pivot = self._var.load(env_value)
        if pivot is not None:
            return pivot
        return env_value

    def _resolve_sar(self, key: str, n: str, model_name: str) -> str:
        gate_env = f"SarModel{n}"
        gate = os.environ.get(gate_env)
        if not gate:
            # Env gate unset → model not whitelisted → empty string.
            return ""
        allowed = {m.strip().lower() for m in gate.split(",") if m.strip()}
        if model_name.lower() not in allowed:
            return ""
        hit = self._sar.load(key)
        # File missing still gates to empty rather than raw token —
        # the gate said this model is whitelisted, so the cascade
        # should substitute, not leak the placeholder.
        return hit if hit is not None else ""

    # ------------------------------------------------------------- lifecycle

    async def __aenter__(self) -> VariableCascade:
        await self.start_watching()
        return self

    async def __aexit__(self, *_exc: object) -> None:
        await self.stop_watching()


__all__ = ["VariableCascade"]
