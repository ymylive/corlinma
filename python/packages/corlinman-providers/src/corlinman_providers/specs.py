"""Provider / alias / embedding configuration specs.

Feature C (§2) pulls the provider wire-up out of a hardcoded prefix table and
into ``config.toml`` — each provider is declared via ``[providers.<name>]``
with a ``kind`` discriminator. This module defines the pydantic shapes the
Rust gateway hands us over whatever channel the Python side learns about
config (today: ``CORLINMAN_PY_CONFIG`` env → JSON file).

Authoritative reference: ``/tmp/corlinman-feature-c-contract.md`` §1 + §2.
"""

from __future__ import annotations

from enum import StrEnum
from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class ProviderKind(StrEnum):
    """Lowercase discriminator identifying the provider wire shape.

    First-party kinds (``anthropic`` / ``openai`` / ``google`` / ``deepseek``
    / ``qwen`` / ``glm``) have bespoke adapters.

    ``openai_compatible`` plus the seven market kinds added in the
    free-form-providers refactor (``mistral`` / ``cohere`` / ``together`` /
    ``groq`` / ``replicate`` / ``bedrock`` / ``azure``) all speak the OpenAI
    wire format and route through :class:`OpenAICompatibleProvider`. They
    are surfaced as named kinds so admin UIs / configs can document operator
    intent without inventing per-kind adapter classes.

    ``bedrock`` and ``azure`` are declared but the runtime currently raises
    ``NotImplementedError`` when one is used — proper SigV4 / deployment-id
    support lands in a follow-up. Operators who need them today should use
    ``kind = "openai_compatible"`` with an explicit ``base_url``.
    """

    ANTHROPIC = "anthropic"
    OPENAI = "openai"
    GOOGLE = "google"
    DEEPSEEK = "deepseek"
    QWEN = "qwen"
    GLM = "glm"
    OPENAI_COMPATIBLE = "openai_compatible"
    MISTRAL = "mistral"
    COHERE = "cohere"
    TOGETHER = "together"
    GROQ = "groq"
    REPLICATE = "replicate"
    BEDROCK = "bedrock"
    AZURE = "azure"
    # new-api (QuantumNous/new-api) sidecar — OpenAI-wire channel pooling
    # manager. corlinman dispatches via the shared OpenAICompatibleProvider;
    # the named kind exists so the admin UI / inspection commands can
    # document operator intent. See ``docs/design/newapi-integration.md``.
    NEWAPI = "newapi"
    # Built-in echo provider — zero-config OpenAI-shape adapter that
    # reverses the last user message. Used by the easy-setup "skip LLM
    # connection" path (Wave 2.2) so new users can land on a working
    # agent without configuring real credentials first.
    MOCK = "mock"


class ProviderSpec(BaseModel):
    """Single ``[providers.<name>]`` entry from ``config.toml``.

    The backend builds exactly one :class:`CorlinmanProvider` instance per
    enabled spec. Disabled specs are retained for admin-listing only.
    """

    model_config = ConfigDict(frozen=False, extra="allow")

    name: str
    """Unique key, e.g. ``"anthropic"`` or ``"my-local-vllm"``."""

    kind: ProviderKind
    """Wire-shape discriminator — selects which adapter class to build."""

    api_key: str | None = None
    """Resolved API key; ``None`` means "no auth" (valid for local gateways)."""

    base_url: str | None = None
    """Optional for first-party; REQUIRED for ``openai_compatible``."""

    enabled: bool = True

    params: dict[str, Any] = Field(default_factory=dict)
    """Provider-level defaults merged below alias-level overrides."""


class AliasEntry(BaseModel):
    """``[models.aliases.<alias>]`` — routes an alias to a provider+model.

    The alias is the *display* / *user* identifier; ``provider`` must match a
    :class:`ProviderSpec` name and ``model`` is the upstream model id passed
    to the vendor SDK.
    """

    model_config = ConfigDict(frozen=False, extra="allow", protected_namespaces=())

    provider: str
    model: str
    params: dict[str, Any] = Field(default_factory=dict)


class EmbeddingSpec(BaseModel):
    """``[embedding]`` — selects provider + model + dimension for embeddings.

    ``provider`` references a ``[providers.<name>]`` key. The provider SDK
    is reused for embeddings when the kind supports it (OpenAI-compatible
    shapes do; Anthropic does not, for example).
    """

    model_config = ConfigDict(frozen=False, extra="allow", protected_namespaces=())

    provider: str
    model: str
    dimension: int
    enabled: bool = True
    params: dict[str, Any] = Field(default_factory=dict)


def list_supported_kinds() -> list[str]:
    """Return every :class:`ProviderKind` value in stable alphabetical order.

    This is the source the admin "add custom provider" UI consumes for the
    protocol-selector dropdown — see ``W-B1`` in
    ``docs/PLAN_PROVIDER_AUTH.md``. Keeping it as a module-level function
    (rather than an enum classmethod) means callers can route through a
    cheap import without instantiating the enum, and the sorted return
    means the UI dropdown order is deterministic across processes /
    Python builds (enum iteration order is declaration order, which is
    fine for code but surprising in a dropdown).
    """
    return sorted(k.value for k in ProviderKind)


__all__ = [
    "AliasEntry",
    "EmbeddingSpec",
    "ProviderKind",
    "ProviderSpec",
    "list_supported_kinds",
]
