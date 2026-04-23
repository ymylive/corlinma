"""Variable cascade — fixed / tar / var / sar resolution.

See :mod:`corlinman_agent.variables.cascade` for the resolution-order
contract. The placeholder engine bridges into this module for every
``{{Var*}}`` / ``{{Tar*}}`` / ``{{Sar*}}`` / built-in fixed key token.
"""

from __future__ import annotations

from corlinman_agent.variables.cascade import VariableCascade

__all__ = ["VariableCascade"]
