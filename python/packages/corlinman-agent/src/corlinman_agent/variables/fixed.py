"""Fixed-variable resolvers for the variable cascade (tier 1).

Fixed variables are computed on demand by a Python callable — they do
not read from disk. The cascade consults this table before anything
else, so any key registered here silently shadows a same-named file in
``TVStxt/tar`` or ``TVStxt/var``.

Two built-ins ship by default:

* ``TimeVar`` — current UTC time, ISO-8601 with seconds precision.
* ``Date`` — today's date (UTC), ``YYYY-MM-DD``.

Callers can override either via :meth:`FixedRegistry.register`.
"""

from __future__ import annotations

from collections.abc import Callable
from datetime import UTC, date, datetime

Resolver = Callable[[], str]


def _default_timevar() -> str:
    # ISO-8601 with a ``Z`` suffix; seconds resolution is enough for
    # prompt templating and keeps the string stable within a render.
    return datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def _default_date() -> str:
    return date.today().isoformat()


class FixedRegistry:
    """Case-sensitive name → resolver table.

    Not thread-safe; the cascade holds one per instance and resolves
    synchronously on the caller's thread.
    """

    def __init__(self) -> None:
        self._resolvers: dict[str, Resolver] = {}
        self.register("TimeVar", _default_timevar)
        self.register("Date", _default_date)

    def register(self, key: str, resolver: Resolver) -> None:
        self._resolvers[key] = resolver

    def has(self, key: str) -> bool:
        return key in self._resolvers

    def resolve(self, key: str) -> str | None:
        r = self._resolvers.get(key)
        if r is None:
            return None
        return r()


__all__ = ["FixedRegistry", "Resolver"]
