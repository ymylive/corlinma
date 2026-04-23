"""Skill-card dataclass — mirrors ``rust/crates/corlinman-skills/src/skill.rs``.

A ``Skill`` is parsed from a ``SKILL.md``-style file: YAML frontmatter
fenced by ``---`` delimiters followed by a Markdown body. The body is
preserved verbatim so downstream prompt injection can paste it without
reformatting surprises.

Only the fields the context assembler actually uses are modelled; other
frontmatter keys are ignored so operators can carry metadata for sister
tooling without breaking our loader.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path


@dataclass(frozen=True)
class SkillRequirements:
    """Runtime prerequisites a skill needs before injection is allowed.

    Every list defaults to empty — an unmet item yields a human-readable
    message from :meth:`SkillRegistry.check_requirements`.
    """

    # All binaries in this list must be on ``$PATH``.
    bins: list[str] = field(default_factory=list)
    # Any ONE binary in this list must be on ``$PATH``.
    any_bins: list[str] = field(default_factory=list)
    # Dotted config keys (e.g. ``providers.brave.api_key``) that must
    # resolve to a non-empty string via the caller-supplied lookup.
    config: list[str] = field(default_factory=list)
    # Environment variables that must be set to a non-empty value.
    env: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class Skill:
    """A single skill parsed from a ``SKILL.md`` file.

    Attributes
    ----------
    name
        Unique identifier. Agents refer to a skill by this name in their
        ``skill_refs`` list.
    description
        Short human summary shown in listings; not injected into prompts.
    emoji
        Optional glyph used by the CLI/UI.
    requires
        Runtime prerequisites enforced before body injection.
    install
        Optional install hint surfaced when ``requires`` isn't satisfied.
    allowed_tools
        Tools this skill is allowed to invoke at runtime. Enforcement
        happens elsewhere; we just carry the list.
    body_markdown
        The Markdown body (everything after the closing ``---`` of the
        frontmatter), preserved verbatim for prompt injection.
    source_path
        Absolute path to the file this skill was loaded from; useful for
        error messages and admin tooling.
    """

    name: str
    description: str
    emoji: str | None = None
    requires: SkillRequirements = field(default_factory=SkillRequirements)
    install: str | None = None
    allowed_tools: list[str] = field(default_factory=list)
    body_markdown: str = ""
    source_path: Path | None = None


__all__ = ["Skill", "SkillRequirements"]
