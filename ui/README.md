# corlinman UI

Next.js admin console for the corlinman gateway.

## Dev

```bash
pnpm install
pnpm dev
```

## API source: mock vs real gateway

The admin pages talk to an API through `lib/api.ts`. Three switches control
where calls land:

| env var | effect |
| --- | --- |
| `NEXT_PUBLIC_GATEWAY_URL` | Real gateway base URL. Default: empty string (use current origin so nginx proxies `/admin/*` through). Set to `http://localhost:6005` for local dev without a proxy. |
| `NEXT_PUBLIC_MOCK_API_URL` | If set, *all* calls go here instead of the gateway (the standalone mock server in `ui/mock/server.ts`). |
| `NEXT_PUBLIC_MOCK_MODE` | `"1"` enables per-call inline mock payloads (offline dev with no mock server and no gateway). Anything else disables them. |

### Run against the real gateway (M6 default)

```bash
# 1. start the gateway (it reads ~/.corlinman/config.toml)
cargo run -p corlinman-gateway

# 2. run the UI against it
NEXT_PUBLIC_MOCK_API_URL= NEXT_PUBLIC_MOCK_MODE= pnpm dev
```

Admin routes (`/admin/*`) require HTTP Basic against
`config.admin.username` + `config.admin.password_hash` (argon2id).
For browser testing, visit `http://localhost:6005/admin/plugins` directly — the
browser prompts for credentials and then the UI at `http://localhost:3000`
picks up the stored creds via `credentials: "include"`.

### Run fully offline (no gateway, inline mocks)

```bash
NEXT_PUBLIC_MOCK_MODE=1 pnpm dev
```

### Run against the standalone mock server

```bash
pnpm mock &    # starts ui/mock/server.ts on :7777
NEXT_PUBLIC_MOCK_API_URL=http://127.0.0.1:7777 pnpm dev
```

## Tests

```bash
pnpm typecheck
pnpm lint
pnpm test        # vitest
pnpm build
```

## Known a11y debt

The full `tests/a11y-audit.test.tsx` runs axe-core against every admin page in jsdom. Two cases are currently skipped — both jsdom infrastructure limits, not real a11y violations:

- **`approvals`** — React 19 + react-query + SSE `setTimeout` cleanup interact to produce `destroy is not a function` on unmount in jsdom. The page itself renders fine in a real browser.
- **`canvas`** — axe cannot descend into a sandboxed iframe when the frame lives in a detached jsdom tree ("Respondable target must be a frame in the current window"). The iframe body is static placeholder HTML; the chrome around it is audited.

Both are covered by the real axe browser CLI in CI. See `tests/a11y-audit.test.tsx` for per-case `skip` reasons.

---

## Tidepool design system

Since v0.4 the admin UI runs on **Tidepool** — a warm-orange glass aesthetic layered over the previous Linear-inspired token set. Both palettes coexist; new code reaches for the `tp-*` family, legacy code using `--primary` / `--accent` now resolves to matched amber values so shipped primitives (the shadcn `<Button>`, `<Dialog>`, etc.) render in-palette without modification.

### Theme attribute + boot sequence

- `<html data-theme="light|dark">` drives colour + token resolution. The `.dark` class is kept in lockstep for backwards compatibility with Tailwind's existing `dark:` variant usage.
- Boot order (matters for no-FOUC):
  1. Inline script in `app/layout.tsx` reads `localStorage["corlinman-theme"]` (fallback: `?theme=` URL param → legacy `theme` key → `dark` default), sets both `data-theme` and the `.dark` class before React hydrates.
  2. `<ThemeProvider>` from `next-themes` (in `components/providers.tsx`) is configured with `attribute={["class","data-theme"]}` + `storageKey="corlinman-theme"` so the post-hydration owner reads the same value the boot script wrote.
  3. The Tidepool `<ThemeToggle>` (in `components/ui/theme-toggle.tsx`) writes through the same key.

URL-first theme param (`?theme=light|dark`) is persisted to storage on load so the boot script and next-themes can't disagree on the next visit.

### Primitives

Under `components/ui/` and `components/admin/`. Every retokened page consumes these; don't recreate variants inline.

| Primitive | Purpose |
|---|---|
| `<GlassPanel variant="soft"\|"strong"\|"subtle"\|"primary">` | Core container surface. `subtle` has no `backdrop-filter` — use inside scroll containers where ≥ 6 blur layers would blow the per-viewport GPU budget. |
| `<AuroraBackground>` | Fixed three-radial + diagonal-linear gradient layer. Mounted once at `app/(admin)/layout.tsx`; reads `--tp-aurora-*` + `--tp-bg-*`. |
| `<ThemeToggle>` | Sun/moon pill. |
| `<StatChip>` | Label + value + optional sparkline + delta. `variant="primary"` adds amber ring/glow + `live` badge. |
| `<FilterChipGroup>` | Pill-style filter tabs — single-select or multi via a discriminated union. |
| `<StreamPill>` | Live/paused/throttled indicator with breathing dot + optional rate suffix. |
| `<LogRow variant="dense"\|"comfortable">` | Shared row primitive for log streams and activity feeds. |
| `<DetailDrawer>` | Inline (non-modal) right-side detail pane with `<DetailDrawer.Section>`. For modal variants use `components/ui/drawer.tsx` (Radix-Dialog). |
| `<JsonView>` | Hand-rolled syntax highlighter for JSON payloads — key/string/number/boolean/comment spans. |
| `<MiniSparkline>` | 6-bar availability/trend viz. |
| `<CommandPalette>` | Configurable ⌘K modal over cmdk. Takes `groups: PaletteGroup[]`; consumers inject actions from any page. |
| `<UptimeStreak>` | Dashboard/health big-number card with 30-bar history. |

### Tokens

All `--tp-*` CSS variables live in `app/globals.css` under the default `:root` block (day) and the `.dark` selector (night). Matching Tailwind tokens exposed in `tailwind.config.ts` as flat classes — `bg-tp-glass`, `text-tp-ink`, `border-tp-glass-edge`, `bg-tp-amber-soft`, `text-tp-amber`, `bg-tp-grad-text` (gradient utility), `shadow-tp-panel / -hero / -primary`, `backdrop-blur-glass / -glass-strong`, etc.

The ambient `--tp-amber` (oklch 0.80/0.17/58 night, 0.56/0.19/50 day) is tuned for decorative/border/glow usage — it does **not** meet WCAG AA contrast for white text. For button backgrounds and any text-on-amber surface, the `--primary` token (hsl `20 82% 33%` day / `35 90% 65%` night with matching `--primary-foreground`) is calibrated to hit AA (4.5:1).

### Motion

- **Continuous** (breathing dots, pulse badges, draw-in underlines, just-now row highlights): CSS keyframes under `.tp-breathe / .tp-breathe-amber / .tp-badge-pulse / .tp-draw-in / .tp-just-now` in `globals.css`. All respect `prefers-reduced-motion: reduce` via the trailing `@media` block.
- **Transient entrance** (stat tick-up, palette open): Framer variants `tickUp` / `paletteIn` in `lib/motion.ts`, paired with instant copies returned by `useMotionVariants()` when reduced-motion is on.

### Performance budget

Backdrop-blur is not free. Current per-viewport worst case (Dashboard) renders ~9 blur layers (sidebar + topnav + hero strong + 4 stat chips + 2 content panes). Target ≤ 5 on older Apple Silicon / Intel integrated GPUs. If profiling shows LCP regressions, shift non-hero surfaces to `<GlassPanel variant="subtle">` (no `backdrop-filter`, solid `rgba` background instead).

### Adding a new retokened page

1. Scaffold with `<GlassPanel variant="strong" as="section">` for the hero, prose summary with inline `<span className="bg-tp-glass-inner-strong border border-tp-glass-edge">` metric chips.
2. Stat row: `<StatChip variant="primary" live>` for the most active metric, `<StatChip>` defaults for the rest.
3. List / activity surface: `<GlassPanel soft>` + `<LogRow variant="dense|comfortable">` (or purpose-specific row).
4. Detail pane: `<DetailDrawer title subsystem meta trace>`, content in `<DetailDrawer.Section label>` wrappers, payload via `<JsonView>`.
5. Offline state: copy the `OfflineBlock` pattern from `app/(admin)/plugins/page.tsx` — it suppresses HTML-dump diagnostics so an upstream gateway 404 page doesn't leak into the shell.
6. i18n: add keys under a `<page>.tp.*` sub-namespace in both `lib/locales/zh-CN.ts` and `lib/locales/en.ts`. Don't touch other `*.tp.*` sub-namespaces — they're owned by their respective pages.

### Migration commit trail

Commits on `feat/tidepool-phase-0` since v0.3.0, in order:

- `phase 0` — tokens + fonts + motion primitives
- `phase 1 wave A/B/C/D` — 12 new primitive components (+51 tests)
- `phase 2` — shell cutover (layout / sidebar / topbar / boot script)
- `phase 3` — Dashboard content
- `phase 3.5` — palette migration
- `phase 4` — Logs + virtualised stream + detail drawer
- `phase 5a–5f` — all 14 remaining admin pages + Login
- `phase 6` — a11y audit (23 pages × 0 serious), contrast hardening, docs

See `_design/migration-plan.md` for the scoping document and `_design/direction-f-tidepool.html` for the original HTML prototype.
