---
name: note-taking
description: Capture decisions, facts, and intermediate findings to durable memory so future turns and future sessions can build on them.
metadata:
  openclaw:
    emoji: "🗒️"
    requires:
      bins: []
      anyBins: []
      config: []
      env: []
    install: |
      No installation needed. The skill uses the built-in `memory.*`
      tools backed by the corlinman vector + KB stores, which are always
      available in-process.
allowed-tools:
  - memory.write
  - memory.search
  - file.read
  - file.write
---
# Note-Taking

## Overview

A long working session generates more facts than you can keep in active context: decisions made, references checked, dead ends ruled out, user preferences stated. Notes are how those facts survive the context window.

**Core principle:** if a fact will matter again later, write it down — once, cleanly, with the *why*. The next turn (or next session) reads it back.

## When to capture a note

- **Decision made** — the user (or you, with consent) chose option A over B. Capture A *and the reason* B was rejected.
- **Fact established** — a config value, a URL, a person's name, a project alias. Anything you'll look up again.
- **Constraint discovered** — "we deploy to k8s, never docker-compose"; "tabs not spaces"; "release freeze starts <date>".
- **Dead end ruled out** — "tried approach X; failed because Y". Saves the next agent from retrying.
- **Intermediate result** — a generated SQL query, regex, JSON schema you'll reference again.

## When NOT to capture

- Trivia that only matters for the current reply.
- Anything sensitive — secrets, API keys, PII. Notes are persisted; treat them like a log file.
- A literal restatement of something the user just said in this turn — the conversation already has it.
- "Maybe useful someday" speculation. If you can't name a *concrete* future turn that needs it, skip.

## Where notes go

Corlinman exposes two persistence surfaces; pick by lifespan:

1. **Session/cross-session memory** — `memory.write(key, value, tags)`. Survives the session and is searchable from later sessions via `memory.search`. Use this for almost everything.

2. **Profile-level markdown** — `MEMORY.md` and `USER.md` in the active profile root. Human-readable, edited by humans and agents, capped around 2 000 chars. Use for distilled, evergreen facts (user role, preferred tools, hard rules) — *not* for transient session notes.

If you're not sure, use `memory.write`. Profile markdown is curated, not stream-captured.

## Anatomy of a good note

```
key:   <stable dotted slug>            # e.g. "project.deploy_target"
value: <one sentence, the fact + why>  # ≤140 chars when possible
tags:  [<session key>, <topic>, ...]   # makes recall queries selective
```

**Good**

```
memory.write(
  key="user.preference.code_review_tone",
  value="Prefers terse, blocker-first reviews — long prose 'suggestions' read as noise.",
  tags=["session-2026-05-18", "code-review", "tone"]
)
```

**Bad**

```
memory.write(
  key="note1",
  value="They like short reviews",
  tags=[]
)
```

The bad version has an unstable key (will collide), no rationale, no tags for recall.

## Hygiene

- **Overwrite, don't duplicate** — reuse the same `key` when a fact changes. The latest write wins.
- **One concept per note** — long blobs don't retrieve cleanly.
- **Tombstone deletes** — if the user says "forget that", `memory.write` the same `key` with an empty value rather than relying on a separate delete path.
- **Tag with the session key** plus a topic — lets later sessions filter "what came out of session X".

## Recall pattern

Before answering a question that might depend on prior state:

```
memory.search("<natural-language query>", top_k=3)
```

If a returned note materially changes your answer, cite it briefly:

> "Based on your earlier note that you deploy to k8s, …"

## Profile markdown editing

If you do touch `MEMORY.md` or `USER.md`:

- Keep it under the configured cap (default ~2 000 chars for MEMORY, ~1 000 for USER).
- Append-then-distill: write the new fact at the bottom, then re-read the whole file and merge / drop stale lines. Don't let it grow unboundedly.
- Never paste sensitive data here either — these files sync to the same backing store as `memory.*`.

## Related skills

- `memory` — the tool-level skill for the underlying read/write surface.
- `deep-research` — long research sessions should be punctuated with notes for the non-obvious findings.
- `brainstorming` — the design doc is the *formal* output; notes are the working-memory trail that produced it.
