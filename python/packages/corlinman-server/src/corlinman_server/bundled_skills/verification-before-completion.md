---
name: verification-before-completion
description: Run the verification command and read its output before claiming any work is complete, fixed, or passing.
metadata:
  openclaw:
    emoji: "✅"
    requires:
      bins: []
      anyBins: []
      config: []
      env: []
    install: |
      No installation needed. The skill enforces a discipline: run the
      command, read the output, then state the result.
allowed-tools:
  - shell.run
---
# Verification Before Completion

## Overview

Claiming work is complete without verification is dishonesty, not efficiency.

**Core principle:** evidence before claims, always.

## The Iron Law

```
NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE
```

If you haven't run the verification command *in this message*, you cannot claim it passes.

## The gate

Before claiming any status or expressing satisfaction:

1. **Identify** the command that proves the claim.
2. **Run** it — full command, fresh, complete output captured.
3. **Read** the full output — check exit code, count failures.
4. **Verify** the output actually supports the claim.
5. **Only then** state the result, with the evidence next to it.

Skip any step = lying, not verifying.

## Common failures

| Claim | Requires | Not sufficient |
|-------|---------|----------------|
| Tests pass | Test command output, 0 failures | Previous run, "should pass" |
| Linter clean | Linter output, 0 errors | Partial check, extrapolation |
| Build succeeds | Build command, exit 0 | Linter passing, logs "look good" |
| Bug fixed | The original failing test now passes | Code changed, assumed fixed |
| Regression test works | RED-GREEN cycle verified | Test passes once |
| Subagent completed | `git status`/`git diff` shows changes | Agent reports "success" |
| Requirements met | Line-by-line checklist | Tests passing |

## Red flags — STOP

You are about to violate this rule if you find yourself:

- Using "should", "probably", "seems to".
- Saying "Done!" / "Perfect!" / "Great!" before running the check.
- About to commit, push, or open a PR without verification.
- Trusting a subagent's success report without independent verification.
- Thinking "just this once" or "I'm tired".

## Rationalisation prevention

| Excuse | Reality |
|--------|---------|
| "Should work now" | RUN the verification. |
| "I'm confident" | Confidence ≠ evidence. |
| "Linter passed" | Linter ≠ compiler. |
| "Subagent said success" | Verify independently. |
| "Partial check is enough" | Partial proves nothing. |

## Key patterns

**Tests** — run the suite, see N/N pass, then say "all tests pass".

**Regression tests (TDD red-green)** — write → run (pass) → revert the fix → run (MUST FAIL) → restore → run (pass). Without the red phase, the test proves nothing.

**Subagent delegation** — agent reports success → check `git diff`/`git status` → verify the change exists → report actual state.

**Requirements** — re-read the plan → build a checklist → tick each item against the diff → report gaps or completion.

## The bottom line

Run the command. Read the output. *Then* claim the result. No shortcuts.

## Related skills

- `systematic-debugging` — the upstream discipline that produces a fix worth verifying.
- `test-driven-development` — TDD's verify step uses this skill.
- `requesting-code-review` — runs the verification pipeline before commit.
