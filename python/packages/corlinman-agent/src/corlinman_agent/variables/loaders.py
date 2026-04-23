"""File-backed loaders for the tar/var/sar tiers of the variable cascade.

Each tier corresponds to a directory of ``<stem>.txt`` files. The
cascade accesses them through :class:`DirLoader`, which keeps a small
content cache keyed by stem and invalidates on demand (either from the
hot-reload watcher or because the caller explicitly cleared it).

Design notes
------------

* Content is read with ``utf-8`` and right-stripped of a single trailing
  newline — prompt fragments almost always end in ``\\n`` on disk but
  inline substitution should not leak that.
* Missing files return ``None`` (distinct from empty string); the
  cascade uses that to fall through to the next tier.
* The loader is synchronous; hot-reload is the only async surface.
"""

from __future__ import annotations

from pathlib import Path


class DirLoader:
    """Read ``<stem>.txt`` files from ``root`` with in-memory caching."""

    def __init__(self, root: Path | None) -> None:
        self._root = root
        self._cache: dict[str, str] = {}

    @property
    def root(self) -> Path | None:
        return self._root

    def path_for(self, stem: str) -> Path | None:
        if self._root is None:
            return None
        return self._root / f"{stem}.txt"

    def load(self, stem: str) -> str | None:
        """Return file contents for ``stem`` or ``None`` if absent."""
        if self._root is None:
            return None
        if stem in self._cache:
            return self._cache[stem]
        path = self.path_for(stem)
        if path is None or not path.is_file():
            return None
        text = path.read_text(encoding="utf-8")
        # Strip exactly one trailing newline — anything more is
        # intentional blank lines at EOF that the author wants kept.
        if text.endswith("\n"):
            text = text[:-1]
        self._cache[stem] = text
        return text

    def invalidate(self, stem: str) -> None:
        self._cache.pop(stem, None)

    def invalidate_all(self) -> None:
        self._cache.clear()

    def snapshot_mtimes(self) -> dict[str, float]:
        """Map stem → mtime for every ``*.txt`` under ``root``.

        Used by the hot-reload watcher to diff against its previous
        snapshot. Returns an empty dict if ``root`` is unset or does
        not exist.
        """
        if self._root is None or not self._root.is_dir():
            return {}
        out: dict[str, float] = {}
        for child in self._root.iterdir():
            if child.is_file() and child.suffix == ".txt":
                try:
                    out[child.stem] = child.stat().st_mtime
                except OSError:
                    # Race: file vanished between iterdir and stat.
                    continue
        return out


__all__ = ["DirLoader"]
