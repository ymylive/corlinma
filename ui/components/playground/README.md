# Protocol Playground — A11y contract (coordination note)

This directory is owned by **B3-FE1** (Protocol Playground with streaming
output). **B3-FE5 (Accessibility Auditor)** seeded this README to freeze the
ARIA contract before B3-FE1's components land, so the parallel batches
don't drift.

If you're B3-FE1 and you're about to add `token-stream.tsx` /
`split-pane.tsx`, please honour the markup below. If the files already exist
when you read this, feel free to delete the README — the contract is in
code.

## `token-stream.tsx`

The container that receives incrementally-streamed LLM tokens **must**:

```tsx
<div
  role="log"
  aria-live="polite"
  aria-atomic="false"
  aria-relevant="additions"
  aria-label="Model output"
  data-testid="token-stream"
>
  {tokens.map((t, i) => <span key={i}>{t}</span>)}
</div>
```

Why:

- `role="log"` + `aria-live="polite"` lets VoiceOver / NVDA announce new
  tokens **without** interrupting the user's current reading position.
- `aria-atomic="false"` means the SR re-reads only the added chunk, not the
  entire buffer on every token — otherwise a 200-token response replays from
  the top on every tick.
- `aria-relevant="additions"` narrows announcements to appended nodes (we
  never remove tokens, but stating this keeps JAWS quiet about DOM mutations
  unrelated to the stream).
- Do **not** set `aria-busy` while streaming; it silences the log entirely
  on some screen readers.

## `split-pane.tsx`

The draggable divider between the two panes **must** expose its current
ratio as a slider so keyboard + AT users can resize:

```tsx
<div
  role="separator"
  aria-orientation="vertical"            // or "horizontal"
  aria-label="Resize playground panes"
  aria-valuenow={Math.round(ratio * 100)} // 0–100
  aria-valuemin={10}
  aria-valuemax={90}
  tabIndex={0}
  onKeyDown={handleArrows /* ← / → adjusts ratio by 5% */}
/>
```

Why:

- `role="separator"` + `aria-valuenow` is the WAI-ARIA pattern for
  resizable splitters. `aria-valuenow` **must** update on every drag /
  keystroke so SR users hear the new value.
- Arrow keys (←/→ for vertical, ↑/↓ for horizontal) must adjust the ratio
  by a fixed step (5% is the convention) so keyboard users can use it
  without a pointer.
- The separator must be focusable (`tabIndex={0}`) and must show the shared
  focus ring (`focus-visible:ring-1 focus-visible:ring-ring`).

## Reduced motion

Any token fade-in / divider snap animations must be guarded with
`useMotion()` from `components/ui/motion-safe.tsx`. Under
`prefers-reduced-motion: reduce` the container should append tokens
instantly and the divider should jump without a transition.
