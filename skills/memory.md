---
name: memory
description: Persist short notes for the current session and retrieve them later via the vector store.
metadata:
  openclaw:
    emoji: "🧠"
    requires:
      bins: []
      anyBins: []
      config: []
      env: []
    install: |
      No installation needed. The skill uses the built-in `memory.write`
      and `memory.search` tools backed by the corlinman vector service,
      which is always available in-process.
allowed-tools:
  - memory.write
  - memory.search
---
# Memory

Give the agent a working memory that survives across turns in the same session
and can be recalled on demand later in the conversation.

## When to use

- The user stated a **fact about themselves or their project** that will stay
  relevant for the rest of the conversation (e.g. "I deploy to k8s, never
  docker-compose", "our code style uses tabs").
- The user finished a multi-step decision and you want to pin the outcome so
  the next turn doesn't relitigate it.
- A long reasoning chain produced an intermediate result (a SQL query, a
  regex, a JSON schema) that future turns are likely to reference.

## When NOT to use

- For trivia that only matters for the current reply — just answer and move on.
- For secrets (passwords, API keys, PII). Memory is persisted; do not store
  anything you would not write to a log file.
- As a substitute for the conversation history. If the user literally just
  said it, you don't need to re-save it.

## Workflow

1. **Write**: call `memory.write` with:
   - `key`: short, stable identifier (`user.tz`, `project.deploy_target`).
   - `value`: the fact in one sentence.
   - `tags`: optional list; use the current session key + a topic.
2. **Search**: before answering a question that might depend on prior state,
   call `memory.search` with a natural-language query and `top_k = 3`.
3. **Cite**: if a retrieved note changes your answer, mention it briefly:
   "Based on your earlier note that you deploy to k8s, ...".

## Hygiene

- Overwrite, don't duplicate: reuse the same `key` when a fact changes.
- Prefer one note per concept; long blobs are harder to retrieve cleanly.
- If the user says "forget that" about a specific fact, call `memory.write`
  with the same `key` and an empty value to tombstone it.
