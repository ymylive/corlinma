"""Stdlib-only hot-reload watcher for the variable cascade.

We intentionally do *not* pull in ``watchdog``: it is not already a
dependency of ``corlinman-agent`` (see ``pyproject.toml``) and the
footprint of the cascade directories — a few dozen small text files —
does not warrant a native OS-event fanout. A once-per-second mtime
scan is plenty.

The watcher is an ``asyncio`` task. For every tracked
:class:`DirLoader` it holds a snapshot of ``stem -> mtime`` and, on
each tick, diffs against a fresh snapshot. Any stem whose mtime
changed (or disappeared) is invalidated on the loader so the next
:meth:`DirLoader.load` call re-reads from disk.
"""

from __future__ import annotations

import asyncio
import contextlib
from collections.abc import Iterable

from corlinman_agent.variables.loaders import DirLoader

_DEFAULT_INTERVAL = 1.0


class HotReloadWatcher:
    """Polls tracked loaders and invalidates their caches on change."""

    def __init__(
        self,
        loaders: Iterable[DirLoader],
        interval: float = _DEFAULT_INTERVAL,
    ) -> None:
        self._loaders: list[DirLoader] = [ld for ld in loaders if ld.root is not None]
        self._interval = interval
        self._task: asyncio.Task[None] | None = None
        self._snapshots: list[dict[str, float]] = [
            loader.snapshot_mtimes() for loader in self._loaders
        ]

    async def start(self) -> None:
        if self._task is not None:
            return
        # Re-snapshot at start so changes applied between construction
        # and start() don't trigger spurious invalidations.
        self._snapshots = [loader.snapshot_mtimes() for loader in self._loaders]
        self._task = asyncio.create_task(self._run(), name="variable-cascade-hot-reload")

    async def stop(self) -> None:
        task = self._task
        if task is None:
            return
        self._task = None
        task.cancel()
        # Swallow cancellation and any last-tick IO errors: stop() is
        # always best-effort.
        with contextlib.suppress(asyncio.CancelledError, Exception):
            await task

    async def _run(self) -> None:
        try:
            while True:
                await asyncio.sleep(self._interval)
                self._tick()
        except asyncio.CancelledError:
            raise

    def _tick(self) -> None:
        for idx, loader in enumerate(self._loaders):
            prev = self._snapshots[idx]
            cur = loader.snapshot_mtimes()
            # Stems present-then-changed or present-then-missing need
            # invalidation; brand-new stems don't need it (loader cache
            # has no entry to evict yet).
            for stem, mtime in prev.items():
                if cur.get(stem) != mtime:
                    loader.invalidate(stem)
            self._snapshots[idx] = cur


__all__ = ["HotReloadWatcher"]
