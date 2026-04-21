# openclaw → corlinman gap analysis & roadmap

_Generated 2026-04-21. Authoritative until superseded. Read-only inventory; no
code was written as part of this analysis._

## Summary

| Bucket | Count | Notes |
| --- | --- | --- |
| Total openclaw plugins inventoried | 99 | under `openclaw/VCPToolBox/Plugin/*` |
| Subsystems (non-plugin) | 7 | DailyNote memory, VCPTimedContacts, VCPChrome, SillyTavernSub, OpenWebUISub, AdminPanel, vcp-installer-source |
| Agent personas | 6 | `.txt` free-form prompt files |
| **Already covered in corlinman** | 12 | provider routing, RAG, channels, scheduler, admin UI, doctor, plugin runtime… |
| **MUST HAVE** | 9 | memory/diary system, FileOperator, ChromeBridge, UrlFetch, PowerShell/Shell executor, AgentAssistant, VCPTavern, DeepMemo, PaperReader |
| **SHOULD HAVE** | 16 | BilibiliFetch, ArxivDaily, DeepWikiVCP, DailyHot, weather, GoogleSearch, TavilySearch, SerpSearch, FreeWebSearch, XiaohongshuFetch, ScheduleManager, SciCalculator, TencentCOSBackup, KarakeepSearch, ProjectAnalyst, CodeSearcher |
| **NICE TO HAVE** | ~30 | image gen (Flux / Doubao / NAI / ZImage / Gemini / Qwen / NanoBanana…), video gen, anime/tarot, MIDI, SVCardFinder, JapaneseHelper, KEGG, NCBI, PubMed… |
| **SKIP** | ~25 | VCPForum ecosystem, UserAuth CAPTCHA, Randomness, FRPSInfo, 1PanelInfo, Agent-specific lore plugins, SillyTavernSub, OpenWebUISub user-scripts, installer .exe, duplicates of corlinman built-ins |
| Sprints proposed (S9–S18) | 10 | ≈ 21 FT weeks |

Openclaw is a **memory-first Node.js playground** where an "AI Maid household"
writes diaries, invokes 99 plugins through a hand-crafted Chinese-bracketed
text protocol (`<<<[TOOL_REQUEST]>>>` with `key:「始」value「末」`), and renders
results in bespoke front-ends (VCPChat, SillyTavern, OpenWebUI). Everything
that matters to corlinman sits in three places:

1. **The memory system.** `DailyNote + RAGDiaryPlugin + TagMemo "浪潮"
   algorithm + EPAModule + ResidualPyramid + DeepMemo` — a richer memory
   stack than corlinman's hybrid RAG.
2. **A dozen genuinely useful tool plugins** (FileOperator, ChromeBridge,
   UrlFetch, PaperReader, ScheduleManager, SciCalculator, various search
   aggregators).
3. **The timed-contact + cross-agent orchestration layer** (VCPTimedContacts,
   AgentAssistant, ScheduleBriefing) — sugar on top of what corlinman's
   scheduler already provides, but with delightful UX.

Everything else is either already built in corlinman (plugin runtime, auth,
admin UI, channels, doctor, metrics) or domain-specific (image gen cluster,
specialty scientific search, anime lookup, tarot).

---

## 1. Covered in corlinman (no action needed)

- **OpenAI-compatible `/v1/chat/completions` + streaming SSE** — corlinman's
  gateway is the authoritative implementation; openclaw's `server.js` is the
  prior art reference point.
- **Tool-call loop + per-model retries / model aliases / ModelRedirect** —
  gateway implements this; openclaw's `MaxVCPLoop*` config is just a number
  in our `[providers]` section.
- **Plugin runtime with sync / async / service types** — already 3 types; plus
  Docker sandbox, which openclaw lacks. Openclaw's 6th type (`hybridservice`)
  is functionally covered by our service plugins that also expose static
  placeholders via `/admin/rag` context injection (see translation concerns
  in §7.2).
- **Hybrid RAG (HNSW + BM25 + RRF + optional rerank)** — openclaw's
  RAGDiaryPlugin/TagMemo is a richer memory *abstraction* on top of a
  similar retrieval stack. Core search is covered; memory semantics are the
  §2 MUST-HAVE.
- **QQ OneBot v11 + Telegram channels** — corlinman ships both; openclaw
  only ships QQ via the (ported separately) `qqBot.js`.
- **Scheduler (tokio-cron-scheduler)** — covers openclaw's `taskScheduler.js`
  + VCPTimedContacts *mechanism*. The *UX* of "schedule a future tool call"
  is the gap (see §3 ScheduleManager).
- **Admin UI (Linear-style)** — replaces openclaw's AdminPanel in full.
  Openclaw AdminPanel is `<script src>`-era static HTML with marked.min.js;
  our Next.js console is strictly ahead.
- **OTel + Prometheus + Grafana dashboard** — openclaw has nothing here.
- **21-check doctor** — openclaw has nothing here.
- **Config hot-reload + Monaco editor + JSON-schema validation** — covered.
- **Auth (cookie session)** — covered. Openclaw's `UserAuth` plugin
  (6-digit rotating code) is a different (weaker) auth model we do not want.
- **Plugin approval gate (SSE + SQLite)** — covered; openclaw has no
  equivalent.
- **FileFetcherServer (distributed file hop)** — not needed: corlinman's
  single-binary deployment model makes openclaw's WebSocketServer node
  fan-out irrelevant. If multi-node ever lands it goes in the post-1.0 P2
  "distributed mode" track, not here.

---

## 2. MUST-HAVE (roadmap priority)

### 2.1 Memory / Diary system (DailyNote + RAGDiaryPlugin + TagMemo)

**Functionality.** Each agent has a persistent "diary" — timestamped,
Tag-labelled, author-signed markdown entries written **by the agent itself**
as a tool call. Entries are embedded + indexed by a tag-aware hybrid retriever
(TagMemo "浪潮" algorithm: tags act as semantic gravitational anchors that
warp vector space). A "Memory Master" sub-agent (backed by a strong model)
auto-fills missing tags. Diaries are per-agent, per-group, and
cross-agent-shared via directory conventions. Retrieval is injected via
`{{AgentName日记本}}` / `[[日记本::Time::Group::TagMemo0.55]]` placeholders
in system prompts; access patterns include "all" (whole-diary context
injection), "TagMemo" (tag-weighted RRF), "Rerank", "Time" (time-decay).

**Effort.** L (3–5 days) for core write/read/retrieve loop; XL (1w+)
including TagMemo wave algorithm + placeholder language + Memory Master
auto-tagging + per-agent/group isolation + migration from plain RAG chunks.

**Mapping.** New subsystem `corlinman-memory` crate depending on
`corlinman-vector`, with:
- `sync` plugin `dailynote-write` (create / update / edit / delete entry)
- `builtin` placeholder resolver (system-prompt-time expansion of
  `{{AgentName日记本}}`, `[[... ::TagMemo0.55]]`)
- optional `async` plugin `dailynote-retag` (Memory Master)

**Integration points.**
- New `[memory]` config section (diary root dir, per-agent dir rules,
  default access mode, tag namespace).
- `python/packages/corlinman-agent` injection hook in system-prompt
  composition path.
- `corlinman-vector` extended with "tag-weighted scoring" option (TagMemo
  is essentially BM25 + tag-edge-boosting + RRF with a post-rerank on
  tag-matched subset).

**Translation concerns.** Openclaw diaries are written by AI via a VCP text
marker; corlinman plugins speak OpenAI tool_calls. Clean translation: the
DailyNote plugin exposes `write`, `update`, `delete` tools; agent invokes
through normal tool-call path. Placeholder expansion (`{{AgentName日记本}}`)
needs a Tera/MiniJinja-style renderer in the prompt-composition step.

---

### 2.2 FileOperator plugin (read/write/search/convert)

**Functionality.** 16 commands: `ReadFile / WebReadFile / WriteFile /
WriteEscapedFile / AppendFile / EditFile / ListDirectory / FileInfo /
CopyFile / MoveFile / RenameFile / DeleteFile / CreateDirectory /
SearchFiles / DownloadFile / ApplyDiff / UpdateHistory / CreateCanvas`.
Auto-extracts text from PDF / DOCX / XLSX / CSV. Scoped by
`ALLOWED_DIRECTORIES` allowlist + `MAX_FILE_SIZE`. This is the "swiss army
knife" plugin; 60% of AI Maid use-cases depend on it.

**Effort.** M (1–2 days). Mostly boilerplate around `tokio::fs` +
[`pdf-extract`] / [`docx-rs`] / [`calamine`] crates. `ApplyDiff` /
`UpdateHistory` need care for idempotence.

**Mapping.** `sync` plugin. Ship in `rust/crates/corlinman-plugins-builtin/`
(new crate) so it boots with gateway.

**Integration.** New `[plugins.file_operator]` config (allowlist, max size).
Uses existing approval gate for `DeleteFile / WriteFile` under
`mode = "prompt"`.

**Translation concerns.** Openclaw's `「始」「末」` brackets → OpenAI tool-call
args JSON object. The `UpdateHistory` tool that edits chat-history JSON is
openclaw-client-specific; skip or re-spec to "edit any text file's
assistant-role section".

---

### 2.3 ChromeBridge plugin (browser control)

**Functionality.** Chrome extension + backend plugin. AI reads current page
as simplified Markdown (`{{VCPChromePageInfo}}` placeholder) and issues
`type` / `click` / `open_url` commands that wait for page refresh before
returning new DOM. Hybrid plugin: service side holds a WebSocket to the
extension, synchronous side executes commands.

**Effort.** L (3 days). Must port the Chrome extension (MV3, `background.js`
+ `content_script.js` ≈ 400 LoC) and bridge it to corlinman's gateway via
a new `/plugin-ws/chrome` endpoint.

**Mapping.** `service` plugin (holds WS) + Chrome extension as a separate
artifact under `ui/extensions/chrome-bridge/`.

**Integration.** New `[plugins.chrome_bridge]` config (WS auth token, per-AI
isolation). Extension uses `manifest_version: 3` + `host_permissions:
<all_urls>`.

**Translation concerns.** Placeholder `{{VCPChromePageInfo}}` needs the
prompt-composition placeholder resolver from §2.1. Consider whether we
expose this as a standing system-prompt section (openclaw-style) vs. a tool
the agent can call when it decides it wants to see the page (more
idiomatic for tool-calling agents). Recommend the latter + keep the
placeholder for compatibility.

---

### 2.4 UrlFetch plugin (fetch arbitrary URL)

**Functionality.** `text` (parsed text + link list) / `snapshot` (full page
screenshot) / `image` (download image as base64). General-purpose web
content ingestion; used by virtually every research workflow in openclaw.

**Effort.** S (half day for text mode, +half for snapshot via
headless-chrome / playwright).

**Mapping.** `sync` plugin (text / image) + `async` (snapshot, because
headless browser takes seconds).

**Integration.** Ship in `corlinman-plugins-builtin`. `[plugins.url_fetch]`
config for timeout / user-agent / proxy.

**Translation concerns.** None significant. Openclaw's base64 image return
maps onto corlinman's existing multimodal attachment convention from S4.

---

### 2.5 LinuxShellExecutor + PowerShellExecutor (system command execution)

**Functionality.** Multi-layer safe shell execution. LinuxShellExecutor has
"six-layer security" (cmd allowlist, path sandbox, resource limits, output
truncation, timeout, audit log), supports async tasks + interactive-block
detection + LinuxLogMonitor cross-plugin integration. PowerShellExecutor is
the Windows equivalent.

**Effort.** L (3 days for Linux + M (1d) for PowerShell). Security design
is the hard part, not the spawn() code.

**Mapping.** `sync` plugin with hard-coded sandbox (prefer Docker sandbox
from corlinman's existing sandbox layer + approval gate ON by default).

**Integration.** `[plugins.shell_exec]` config (allowlist / blocklist /
timeout / max-output). Must be `approval = "prompt"` by default. Runs in
our existing Docker sandbox, with `cap_drop=ALL`, `read_only_root=true`
unless explicitly relaxed per-command.

**Translation concerns.** Skip PowerShell on first pass (Linux server
deployment is the 1.0 target). Openclaw's six-layer validator classes
(`Plugin/LinuxShellExecutor/validators/`) are good prior art — port the
*rules* (regex allowlist, env scrubbing), re-implement in Rust.

---

### 2.6 AgentAssistant (sub-agent delegation / future-call)

**Functionality.** A *tool* that lets the current agent call a *different*
agent as if making a phone call. Supports immediate, scheduled
(`timely_contact: YYYY-MM-DD-HH:mm`), and async-delegation
(`task_delegation: true → delegationId`) modes. Each sub-agent has its own
memory + persona + tool set. This is "multi-agent" done the pragmatic way:
no swarm framework, just tool calls between agents.

**Effort.** M (1–2 days). Most of the machinery already exists in
corlinman: scheduler for timed calls, session store for sub-agent context,
reasoning loop for the actual execution. What's missing is the
orchestration primitive.

**Mapping.** `sync` plugin that calls back into the gateway's own
`/v1/chat/completions` with a different agent config (persona + tools +
memory bindings). `async` variant for `task_delegation`.

**Integration.** New `[agents.assistants]` section in config listing
available sub-agents. UI: new "Agents" subpage showing
active delegations.

**Translation concerns.** Openclaw's "7 Maids" (小娜 / 小克 / 小吉 / 小冰 /
小雨 / 小绝 / 小芸) are personality fixtures; in corlinman, these become
generic `agent_name` entries with per-agent system-prompt + tool allowlist.
Recommend we ship **2 example sub-agents** ("researcher" + "executor") as
starter templates, not the Maid roster.

---

### 2.7 VCPTavern (visual context injection editor)

**Functionality.** SillyTavern-style "world info" — a visual rule editor
where conditions like "message contains X" or "depth ≤ N" trigger
injection of extra context into the system prompt. Hybridservice plugin
with a custom HTML editor UI under `AdminPanel/vcptavern_editor.html`.

**Effort.** M (2 days). Data model is straightforward; the UI is the work.

**Mapping.** `builtin` feature in `corlinman-agent` (prompt-composition
layer) + new admin UI page `ui/app/(admin)/context-rules/page.tsx`.

**Integration.** New `[context_rules]` config (or SQLite table for
rule-level editing from the UI). Rules fire in the same pipeline as
placeholder expansion.

**Translation concerns.** Openclaw's SillyTavern heritage means rule syntax
expects `{{user}}` / `{{char}}` etc. — corlinman already uses `{{Agent*}}`
/ `{{Var*}}` placeholders; unify into one namespace before importing rule
sets.

---

### 2.8 DeepMemo (chat-history retrieval engine)

**Functionality.** Rust-based historical-conversation retriever using
Tantivy + jieba-rs tokenizer + custom weighting + swiss-round reranking.
Advanced query language: `VCP, "exact phrase", (重要概念:1.5), [闲聊],
{破解|渗透:1.3}, @30d`. Indexes the user's past chat archives (VCPChat
local JSON files) and lets the agent "recall" specific past exchanges.

**Effort.** L (4 days) to port the query language + scoring; our existing
SQLite-FTS5 BM25 covers the retrieval floor.

**Mapping.** `sync` plugin + new `corlinman-vector::query_lang` module for
the weighted-term parser.

**Integration.** Reads corlinman's session store (introduced in S1.T4) +
optional user-supplied transcript imports. New CLI: `corlinman memory
recall "query string"`.

**Translation concerns.** Openclaw indexes **VCPChat** local files
(client-side archive); corlinman's session store is server-side. The
functional primitive is the same (retrieve past turns by content),
implementation targets our data shape.

---

### 2.9 PaperReader (PDF ingest + deep read + query)

**Functionality.** Rust stdio plugin. `IngestPDF` extracts + chunks a PDF,
`Read` / `ReadSkeleton` / `ReadDeep` progressively deeper reads, `Query`
retrieves chunks matching a question. Pipeline: chunker → ingest → deep-
reader → query. Serves the "research assistant" workflow.

**Effort.** M (2 days) — the hard work (Rust closeout version) exists
upstream; we just re-author the manifest + adapt the corpus to corlinman's
vector store.

**Mapping.** `sync` plugin. Ships under `corlinman-plugins-builtin`.

**Integration.** Reuses `corlinman-vector` for chunk indexing. No new
config section; `[rag]` default applies.

**Translation concerns.** Ingested chunks should live in a dedicated
namespace (`rag.namespace = "papers"`) so they don't contaminate the
diary / general-knowledge indices. Needs the "namespace" primitive added
to `corlinman-vector` first (small — S-sized extension of S3 work).

---

## 3. SHOULD-HAVE

Short form: description / effort / mapping.

### 3.1 ScheduleManager
User-facing schedule CRUD (view / add / delete). Agent-facing tool; the
corlinman scheduler already does cron jobs — this is the **user agenda**
(reminders, events, future hooks). **S** · `sync` plugin + new
`[schedule]` SQLite table in vector DB file. UI page under
`(admin)/schedule`.

### 3.2 SciCalculator
`integral('x*sin(x**2)')` etc. Expression evaluator with calculus + stats.
**XS** · `sync` plugin; [`meval-rs`] / [`sympy` in Python subprocess].

### 3.3 GoogleSearch / TavilySearch / SerpSearch / FreeWebSearch
Four web-search backends. Recommend shipping **Tavily** (best quality) +
**FreeWebSearch** (no-API-key fallback, multi-engine aggregator). **S each.**
`sync` plugins.

### 3.4 UrlFetch snapshot mode
(Already listed as §2.4 MUST; the snapshot variant is SHOULD if §2.4 is
text-only first.)

### 3.5 BilibiliFetch
Video info (subtitles / danmaku / comments / HD frame extraction), keyword
search (videos / UPs), UP submission list. Long-link + b23.tv + multi-P.
**M** · `sync` plugin. High user value for the Chinese user base.

### 3.6 XiaohongshuFetch
Scraper for XHS notes. **S** · `sync`. Fragile (anti-bot) but widely asked
for.

### 3.7 ArxivDailyPapers / CrossRefDailyPapers
Static daily-paper feed into a placeholder. **S each** · `static`
(scheduled periodic fetch) + placeholder.

### 3.8 DeepWikiVCP
Fetch `deepwiki.com` content and convert to Markdown. **S** · `sync`.

### 3.9 DailyHot (56+ hot-news sources)
Trending topics across platforms; injected via placeholder. Useful as a
"current events" context primitive. **M** · `static` + fetcher scheduler.
Large surface (71 source files in openclaw).

### 3.10 WeatherReporter + WeatherInfoNow
Real-time weather injected as `{{VCPWeatherInfo}}`. **S** · `static` +
placeholder.

### 3.11 KarakeepSearch
Bookmark full-text search (Karakeep self-hosted). **S** · `sync`. Useful if
user runs Karakeep.

### 3.12 CodeSearcher (ripgrep wrapper)
Rust-based fast code-content search in configured workspace dirs. **XS** ·
`sync`. Overlaps with external `rg` but value is the AI-friendly tool
schema.

### 3.13 TencentCOSBackup
Full COS CRUD + compression. **M** · `sync`. Replace with S3-compatible
generic adapter for broader usefulness.

### 3.14 VSearch (semantic-level concurrent search)
"Micro-model concurrent semantic search" — uses a small embedding model
for query expansion + parallel candidate ranking. **M** · `sync`. Useful
adjacent to §2.8 DeepMemo.

### 3.15 ProjectAnalyst
Analyzes a project folder (file tree + entry points + language stats) and
emits a digest. **S** · `sync`.

### 3.16 ThoughtClusterManager
AI self-editing "thought chain" files for meta-self-learning. **S** ·
`sync`. Niche but novel capability we should prototype.

---

## 4. NICE-TO-HAVE

| Feature | Brief | Effort |
| --- | --- | --- |
| FluxGen / DoubaoGen / DMXDoubaoGen / NovelAIGen / ComfyUIGen / ComfyCloudGen / GeminiImageGen / QwenImageGen / WebUIGen / ZImageGen[2]/Turbo / NanoBananaGen2/OR | Image generation (11+ variants, each a vendor-specific API wrapper) | M each; pick 2-3 (**FluxGen + DoubaoGen + ComfyUIGen**) |
| Wan2.1VideoGen / GrokVideo / VideoGenerator | Video generation | M each; skip unless user asks |
| SunoGen | Song generation (Suno API) | S |
| TarotDivination | Tarot with astronomy/environment seeding | S |
| AnimeFinder / ArtistMatcher | Anime / SDXL-artist lookup | S each |
| JapaneseHelper | 11-command Japanese grammar / JLPT tool | M |
| KEGGSearch / PubMedSearch / NCBIDatasets | Scientific DB (32+ / 16+ / 22+ commands each) | M each |
| SVCardFinder | 《影之诗》TCG card lookup | S; very niche |
| MIDITranslator | MIDI ↔ text | S |
| IMAPSearch / IMAPIndex | Email indexing | M |
| MagiAgent | Three-sage EVA Magi committee decision | S; fun, not functional |
| LinuxLogMonitor | Event-driven log anomaly detection | M |
| TagFolder | Tag-based file organization | S |
| SemanticGroupEditor | RAG semantic-group edits | S (becomes part of §2.1 memory) |
| PyCameraCapture / PyScreenshot / CapturePreprocessor | Local camera / screenshot | S each; only useful on user's desktop |
| LightMemo | Simpler RAG search (subset of §2.1) | S; absorb into §2.1 |
| MCPO / MCPOMonitor | MCP (Anthropic) bridge + monitor | M; parks in post-1.0 P1 "MCP compat" |
| SnowBridge / VCPToolBridge / VCPForum* | VCP-to-external-runtime bridges + forum | L; openclaw-ecosystem-specific, skip |

---

## 5. SKIP (out of scope for corlinman)

| Feature | Reason |
| --- | --- |
| `VCPForum` / `VCPForumAssistant` / `VCPForumLister` / `VCPForumOnline` / `VCPForumOnlinePatrol` | Openclaw-internal forum + AI-patrol; off-mission |
| `UserAuth` (6-digit rotating CAPTCHA) | Weaker than our session auth; `Randomness` covers the seed |
| `Randomness` | 12 commands for dice/deck; fun but off-core; easy to reintroduce later |
| `FRPSInfoProvider` | FRPS-specific infra info; operator-specific |
| `1PanelInfoProvider` | 1Panel-specific management plane integration |
| `SillyTavernSub/*` | SillyTavern user-scripts; irrelevant to corlinman |
| `OpenWebUISub/*` | OpenWebUI user-scripts; irrelevant to corlinman |
| `vcp-installer-source` | Windows installer; corlinman ships Docker |
| `AdminPanel/` | Openclaw's admin; corlinman has a newer one |
| `VCPLog` service | WebSocket VCP log push; covered by our SSE logs stream |
| `DailyNotePanel` | Embedded DailyNote frontend; covered once §2.1 lands with UI page |
| `AgentDream` / `AgentMessage` | Agent-to-frontend push via WS; covered by corlinman's admin SSE |
| `CapturePreprocessor` | Screenshot placeholder preprocessor; replaced by our multimodal segments |
| `FileServer` / `ImageServer` | Password-protected static file server; nginx does this in prod |
| `EmojiListGenerator` / `FileListGenerator` / `FileTreeGenerator` | Static list injector; covered by placeholder system of §2.1 once it lands |
| `WorkspaceInjector` | Variable workspace dir injector; covered by placeholder system |
| `ImageProcessor` (messagePreprocessor) | Multimodal preprocessor; already covered by S4 multimodal |
| `RiverTestPlugin` | Test fixture |
| `EPAModule` / `ResidualPyramid` / `ResultDeduplicator` (standalone JS) | Internal components of RAGDiaryPlugin; subsumed in §2.1 |
| `SynapsePusher` / `ToolBoxFoldMemo` | Experimental internals, no manifest |

---

## 6. Proposed sprint roadmap (S9–S18)

Baseline: 1.0 shipped (M0–M8 ✅). Follow-on sprints continue sprint
numbering from `docs/roadmap.md` (current last = S8).

**FT weeks = Focus-Time weeks, 4-day work weeks.**

### S9 — Namespaced memory substrate
**Theme.** Prerequisite plumbing for the diary system + PaperReader +
sub-agent memories.
**Tasks (3):**
- S9.T1 · `corlinman-vector` namespace primitive (diary / papers / general)
  — M.
- S9.T2 · Placeholder renderer (Tera/MiniJinja) for
  `{{Var*}} {{Tar*}} {{AgentName日记本}} [[x::TagMemo0.55]]` → wire into
  Python `system_prompt` composition step — M.
- S9.T3 · Session store helper: list/query past turns by agent/session key
  (foundation for §2.8 DeepMemo) — S.

**Estimate.** 1.5 FT weeks. **Deps.** S1.T4 session store (done).
**Parallel?** Solo — unlocks S10 & S12.

---

### S10 — DailyNote memory system (MVP)
**Theme.** The §2.1 core.
**Tasks (5):**
- S10.T1 · `dailynote-write` sync plugin (create / update / delete tool) —
  M.
- S10.T2 · Diary storage convention (`data_dir/memory/<agent>/*.md` with
  YAML frontmatter for tags + author + timestamp) + SQLite metadata
  table — S.
- S10.T3 · Hybrid retriever extension: `TagMemo` scorer (tag-aware RRF) —
  M.
- S10.T4 · Placeholder integration: `{{Agent日记本}}`, `[[x::Time::Group::
  TagMemo0.55]]`, `<<x>>` (all-mode) — S.
- S10.T5 · Admin UI page `(admin)/memory` — list / search / edit entries
  + tag editor — M.

**Estimate.** 2.5 FT weeks. **Deps.** S9. **Parallel?** Yes — can run
alongside S11.

---

### S11 — Tool plugin cluster A (FileOperator + UrlFetch + CodeSearcher + SciCalculator)
**Theme.** Ship the top-used general-purpose tools.
**Tasks (4):**
- S11.T1 · `file-operator` sync plugin (16 commands, minus `UpdateHistory`
  / `CreateCanvas` which are openclaw-client-specific) — M.
- S11.T2 · `url-fetch` sync plugin (text + image modes) — S.
- S11.T3 · `code-searcher` sync plugin (ripgrep wrapper) — XS.
- S11.T4 · `sci-calculator` sync plugin (meval-rs or Python subprocess) — XS.

**Estimate.** 1.5 FT weeks. **Deps.** none. **Parallel?** Yes — S10 ∥ S11
∥ S12.

---

### S12 — Scheduler UX + sub-agents (AgentAssistant + ScheduleManager + TimedContact)
**Theme.** Turn the scheduler into first-class agent UX.
**Tasks (4):**
- S12.T1 · `schedule-manager` sync plugin (user agenda CRUD) — S.
- S12.T2 · `agent-assistant` sync+async plugin (invoke another agent
  as a tool; support immediate / timed / async-delegation modes) — M.
- S12.T3 · Timed tool-call primitive: `POST /admin/schedule/tool_call`
  takes a full `ToolCall` + RFC3339 run-at; scheduler fires the tool and
  delivers result via SSE or push to originating session — M.
- S12.T4 · Admin UI page `(admin)/schedule` — list / cancel / retry — S.

**Estimate.** 2 FT weeks. **Deps.** S11 (for the "what to run" primitive).
**Parallel?** Partially with S13.

---

### S13 — Shell executor (sandboxed)
**Theme.** The single biggest "agent actually does things" unlock.
**Tasks (3):**
- S13.T1 · `shell-exec` sync plugin: takes a command, runs in ephemeral
  Docker container (our existing sandbox), returns stdout/stderr/exit — M.
- S13.T2 · Safe-default rules: command allowlist regex, env scrubbing,
  output truncation, `approval = "prompt"` default — S.
- S13.T3 · Doctor check `shell_exec_sandbox_ok` + integration test with
  `docker run alpine echo hello` — S.

**Estimate.** 1.5 FT weeks. **Deps.** corlinman Docker sandbox (done).
**Parallel?** Yes with S12.

---

### S14 — Web research tools
**Theme.** Search aggregators + external-content ingestion.
**Tasks (5):**
- S14.T1 · `tavily-search` sync plugin — S.
- S14.T2 · `free-web-search` multi-engine aggregator (Bing / DuckDuckGo /
  Brave / Startpage / Wikipedia) — M.
- S14.T3 · `deep-wiki` fetcher (deepwiki.com → Markdown) — S.
- S14.T4 · `bilibili-fetch` (video / UP / search) — M.
- S14.T5 · `arxiv-daily` + `crossref-daily` static placeholders — S
  (combined).

**Estimate.** 2 FT weeks. **Deps.** `url-fetch` from S11.
**Parallel?** Yes with S13.

---

### S15 — Context rules (VCPTavern) + ChromeBridge
**Theme.** Two "soul-of-the-agent" features: visual injection rules + live
browser sense.
**Tasks (4):**
- S15.T1 · Context-rule data model + evaluator in `corlinman-agent`
  prompt-composition pipeline — M.
- S15.T2 · Admin UI page `(admin)/context-rules` — rule editor (Monaco for
  JSONPath expressions) — M.
- S15.T3 · `chrome-bridge` service plugin + `/plugin-ws/chrome` endpoint — M.
- S15.T4 · Chrome extension (MV3) under `ui/extensions/chrome-bridge/`,
  publish-ready zip artifact in CI — M.

**Estimate.** 2.5 FT weeks. **Deps.** S9 placeholder renderer.
**Parallel?** Not easily — touches agent pipeline + a new UI page + a new
browser artifact; owner-intensive.

---

### S16 — DeepMemo + PaperReader
**Theme.** Retrieval power tools.
**Tasks (3):**
- S16.T1 · Query-language parser (weighted terms / exact phrase / exclude /
  OR groups / time filter) in `corlinman-vector::query_lang` — M.
- S16.T2 · `deep-memo` sync plugin (queries session-store + diary + any
  bound archive namespace) — M.
- S16.T3 · `paper-reader` sync plugin (PDF ingest, skeleton/full/deep read,
  query) — M.

**Estimate.** 2 FT weeks. **Deps.** S9 (namespaces) + S10 (diary) + S11
(file ops for PDF load paths). **Parallel?** Yes with S17.

---

### S17 — Content-creation cluster (optional-install image gen)
**Theme.** Image-generation plugins behind a feature flag so the default
distribution stays lean.
**Tasks (4):**
- S17.T1 · Shared plugin scaffold `corlinman-plugin-mediagen` with common
  job/queue/pending primitives — S.
- S17.T2 · `flux-gen` (SiliconFlow) — S.
- S17.T3 · `doubao-gen` (火山引擎) — S.
- S17.T4 · `comfyui-gen` (local ComfyUI API) — M.

**Estimate.** 1.5 FT weeks. **Deps.** S11 file ops (for returning base64
/ file paths). **Parallel?** Yes with S16.

Optional: add `suno-gen` / `grok-video` / `novelai-gen` individually as
post-sprint one-offs — each S-sized.

---

### S18 — Polish + release v0.2.0
**Theme.** Documentation, fixtures, release cycle.
**Tasks (5):**
- S18.T1 · `docs/memory-authoring.md` companion to `plugin-authoring.md` —
  S.
- S18.T2 · Two starter sub-agents (`researcher`, `executor`) with example
  diaries + context rules — S.
- S18.T3 · Migration: fresh diaries + existing RAG chunks moved into new
  namespace — M.
- S18.T4 · CHANGELOG 0.2.0 + release-notes doc + demo screencast — S.
- S18.T5 · `corlinman qa run` adds scenarios `diary-roundtrip`,
  `shell-exec-sandboxed`, `sub-agent-delegation`, `chrome-bridge-smoke`,
  `paper-reader-roundtrip` — M.

**Estimate.** 2 FT weeks. **Deps.** S9–S17. **Parallel?** No — integration
sprint.

---

### Roadmap totals

| Sprint | Theme | FT weeks | Depends on | Parallelizable with |
| --- | --- | --- | --- | --- |
| S9  | Memory substrate | 1.5 | — | — |
| S10 | DailyNote MVP | 2.5 | S9 | S11, S12 |
| S11 | Tools A (file / url / code / calc) | 1.5 | — | S10, S12 |
| S12 | Scheduler UX + sub-agents | 2.0 | S11 | S13 |
| S13 | Sandboxed shell | 1.5 | — | S12 |
| S14 | Web research | 2.0 | S11 | S13 |
| S15 | Context rules + Chrome | 2.5 | S9 | — |
| S16 | DeepMemo + PaperReader | 2.0 | S9, S10, S11 | S17 |
| S17 | Image gen (opt-in) | 1.5 | S11 | S16 |
| S18 | Polish + v0.2.0 | 2.0 | all | — |
| **Total** | | **≈ 21 FT weeks** | | |

With aggressive parallelism (2 engineers/agents), real calendar time ≈ 12
FT weeks to land S9–S18.

---

## 7. Architecture decisions required

These gate the entire roadmap. All must be answered before S9 kicks off.

### 7.1 Protocol compat layer: VCP text markers vs. pure OpenAI tool_calls
**Options.**
- **A. Pure re-author.** Every openclaw plugin we import is re-written
  with an OpenAI-style JSON-schema manifest, invoked via our gRPC/stdio
  JSON-RPC bridge. Clean. Expensive (~2h per plugin to re-manifest).
  Drop `「始」「末」` text markers entirely.
- **B. Compat layer.** A `messagePreprocessor`-style pass that lets agents
  emit `<<<[TOOL_REQUEST]>>>` and the gateway parses it into
  `ToolCall`s before dispatch. Cheap — potentially imports openclaw
  plugins unmodified. But: fragments the tool-calling contract, confuses
  OTel traces, doubles the surface area of plugin authoring.
- **C. Hybrid.** Native path is OpenAI; compat path is *opt-in* per-agent
  for importing legacy openclaw plugins as-is, sandboxed behind a feature
  flag.

**Recommendation: A**, with a one-time import script that shells plugin
manifests over. Consistency beats compat for an 1.0 platform. Revisit C
only if a specific user demands it.

### 7.2 Plugin SDK: keep ours or port openclaw's loader
Openclaw's `Plugin.js` + `PluginManager` has 6 plugin types
(`static` / `messagePreprocessor` / `synchronous` / `asynchronous` /
`service` / `hybridservice`). Our 3 types (`sync` / `async` / `service`)
cover the functional surface; `static` and `messagePreprocessor` map to
our prompt-composition placeholder renderer (S9.T2); `hybridservice` is
`service + static-placeholder` which just means a service plugin that
also registers a placeholder on boot.

**Recommendation: keep ours**. Add an explicit "placeholder provider"
capability declaration in `plugin-manifest.json` (new optional field
`capabilities.placeholders: [{ name: "VCPWeatherInfo", description: "..." }]`
that the resolver reads at system-prompt composition time).

### 7.3 Agent persona format: Markdown frontmatter vs. openclaw `.txt`
Openclaw agents are free-form `.txt` with inline `{{Var*}}` / `[[...]]`
placeholders — good for emotional richness, bad for tooling. Corlinman
uses Markdown with YAML frontmatter (`name`, `model`, `tools`, `memory`).

**Recommendation: keep ours**, add a new frontmatter field
`persona_body_format: "mixed"` that enables placeholder expansion inside
the body. Provide a one-way import script `corlinman agent import <path>.txt`
that turns an openclaw `.txt` into our format (you pick a name; the body
goes verbatim; placeholders stay as-is if the resolver is running).

### 7.4 Memory storage shape: files + SQLite (openclaw) vs. SQLite only
Openclaw writes each diary entry as `.txt` / `.md` under
`dailynote/<agent>/*.md` and indexes metadata in SQLite + usearch — the
files are human-readable authoritative; DB is derived. We could store
everything in SQLite and skip the file layer.

**Recommendation: match openclaw's files-as-truth model.** Operators
appreciate being able to `git add` their agent's memory, grep it with
ripgrep, edit with their editor of choice. Our admin UI reads/writes
the same files. SQLite holds the vector index + metadata cache only.

### 7.5 Sub-agent memory isolation model
Openclaw supports "per-agent" diaries, "group" (shared across a group of
agents), and "public" (read-all-write-all). Our current data model has
no groups.

**Recommendation.** Adopt openclaw's three-tier scheme as-is:
`memory/agents/<name>/` (private) + `memory/groups/<name>/` (shared
within group) + `memory/public/` (read-all). `agents.toml` declares
membership. Compliance: access checked at retrieval time, not at
injection time, so placeholders fail-closed on unauthorized reads.

### 7.6 Chrome extension distribution
Openclaw ships an unpacked MV3 extension. For corlinman:
- **A.** Same: unpacked dev-mode install instructions in docs.
- **B.** CWS (Chrome Web Store) listing: production-quality UX but gates
  behind a developer account + review.

**Recommendation: A for 1.0; B post-1.0.**

### 7.7 Tavern rule language: adopt openclaw's or invent?
Openclaw's VCPTavern + SillyTavern inherit a rule vocabulary
(`constant / activation: normal|conditional`, `depth`, `keywords`).
We could either import it wholesale (network effect + existing worlds
can be imported) or clean-sheet a simpler JSONPath+MiniJinja DSL.

**Recommendation: clean-sheet, but include an "import SillyTavern
cards" converter that translates common rule shapes.**

### 7.8 Plugin approval defaults for shell-exec / file-write
**Options.** `deny` (require user config to enable) vs. `prompt` (ask
on first use).

**Recommendation: `prompt` for `shell-exec`, `file-operator.DeleteFile`,
`file-operator.WriteFile` under paths outside a configured "safe
workspace"; `allow` for `file-operator.ReadFile` and listing commands.**
Default workspace: `data_dir/workspace/`.

---

## 8. Risks & unknowns

- **TagMemo wave algorithm is 500+ LoC of JS with vibe-heavy commentary
  and little algorithmic documentation.** Porting honestly means
  re-deriving the math. Plan 2× the effort; document what we chose to
  skip (EPAModule, ResidualPyramid, SVD dedup) and why.
- **VCPChat client assumptions in openclaw diary display code** leak into
  plugin return schemas (`message_bubble` / placeholder expansions that
  VCPChat renders specially). Our UI must re-implement these or we
  degrade gracefully. Identify all such leaks before S10.
- **Chrome extension MV3 lifetime gotchas.** Service workers get killed
  aggressively; reconnection logic has to be robust. Budget S15 time for
  this.
- **Plugin-ecosystem fragmentation risk.** If we ship FileOperator +
  ChromeBridge + ShellExec as *built-in plugins*, we commit to
  maintaining them in-tree forever. Alternative: ship them as separate
  optional repos. Decide per-plugin; recommend MUST-HAVE items stay
  in-tree (they're platform features), SHOULD-HAVE stays out-of-tree.
- **Docker-sandbox for shell-exec on non-Docker hosts.** What happens on
  bare-metal deployments without Docker? Doctor should flag hard-fail,
  but some operators will want a no-sandbox escape hatch. Decide policy
  before S13.
- **License audit.** Openclaw is MIT (see `LICENSE`); individual plugin
  third-party deps (e.g., `JapaneseHelper` OJAD, `KEGGSearch` KEGG
  ToS) may be incompatible. Must audit each plugin we port for its
  upstream ToS.
- **Chinese-first UX.** Many openclaw features (小红书抓取, 崩铁表情包,
  Maid personas) assume a Chinese-speaking user. Corlinman's admin UI
  already has i18n; plugin display names + tool descriptions should be
  authored bilingually from the start.
- **Persona IP.** Openclaw's Aemeath (鸣潮), Hornet (空洞骑士), Nova are
  fan-fiction characters. Ship **template** sub-agents, not these.

---

## 9. Appendix — openclaw plugin inventory (99 plugins)

Bucket: **C**=Covered, **M**=Must-have, **S**=Should-have, **N**=Nice-to-have,
**X**=Skip.

| # | Plugin | VCP type | Bucket | Mapped to |
| --- | --- | --- | --- | --- |
| 1  | 1PanelInfoProvider | static | X | — |
| 2  | AgentAssistant | hybridservice | **M** | §2.6 → S12.T2 |
| 3  | AgentDream | (no manifest) | X | experimental |
| 4  | AgentMessage | synchronous | X | covered by SSE logs |
| 5  | AnimeFinder | synchronous | N | optional |
| 6  | ArtistMatcher | synchronous | N | optional |
| 7  | ArxivDailyPapers | static | **S** | §3.7 → S14.T5 |
| 8  | BilibiliFetch | synchronous | **S** | §3.5 → S14.T4 |
| 9  | CapturePreprocessor | messagePreprocessor | X | covered by multimodal S4 |
| 10 | ChromeBridge | hybridservice | **M** | §2.3 → S15.T3/T4 |
| 11 | CodeSearcher | synchronous | **S** | §3.12 → S11.T3 |
| 12 | ComfyCloudGen | synchronous | N | optional |
| 13 | ComfyUIGen | synchronous | N | §S17.T4 (included as pilot) |
| 14 | CrossRefDailyPapers | static | **S** | §3.7 → S14.T5 |
| 15 | DailyHot | static | **S** | §3.9 (defer to post-S14) |
| 16 | DailyNote | synchronous | **M** | §2.1 → S10.T1 |
| 17 | DailyNoteManager | (no manifest) | **M** | §2.1 (Memory Master) |
| 18 | DailyNotePanel | service | X | replaced by S10.T5 UI |
| 19 | DailyNoteWrite | synchronous | **M** | §2.1 (internal write path) |
| 20 | DeepWikiVCP | synchronous | **S** | §3.8 → S14.T3 |
| 21 | DMXDoubaoGen | synchronous | N | optional |
| 22 | DoubaoGen | synchronous | N | §S17.T3 (included as pilot) |
| 23 | EmojiListGenerator | static | X | covered by placeholder renderer |
| 24 | FileListGenerator | static | X | covered by placeholder renderer |
| 25 | FileOperator | synchronous | **M** | §2.2 → S11.T1 |
| 26 | FileServer | service | X | nginx in prod |
| 27 | FileTreeGenerator | static | X | covered by placeholder renderer |
| 28 | FlashDeepSearch | synchronous | N | optional |
| 29 | FluxGen | synchronous | N | §S17.T2 (included as pilot) |
| 30 | FreeWebSearch | synchronous | **S** | §3.3 → S14.T2 |
| 31 | FRPSInfoProvider | static | X | operator-specific |
| 32 | GeminiImageGen | synchronous | N | optional |
| 33 | GoogleSearch | synchronous | **S** | §3.3 → S14 (recommend Tavily instead) |
| 34 | GrokVideo | synchronous | N | optional |
| 35 | ImageProcessor | messagePreprocessor | X | covered by multimodal S4 |
| 36 | ImageServer | service | X | nginx in prod |
| 37 | IMAPIndex | (no manifest) | N | optional |
| 38 | IMAPSearch | (no manifest) | N | optional |
| 39 | JapaneseHelper | synchronous | N | niche |
| 40 | KarakeepSearch | synchronous | **S** | §3.11 (post-S14) |
| 41 | KEGGSearch | synchronous | N | niche (biology) |
| 42 | LightMemo | hybridservice | N | absorb into §2.1 |
| 43 | LinuxLogMonitor | asynchronous | N | optional (ops) |
| 44 | LinuxShellExecutor | synchronous | **M** | §2.5 → S13 |
| 45 | MagiAgent | hybridservice | N | gimmick |
| 46 | MCPO | (no manifest) | N | post-1.0 MCP compat |
| 47 | MCPOMonitor | (no manifest) | N | post-1.0 MCP compat |
| 48 | MIDITranslator | (no manifest) | N | niche |
| 49 | NanoBananaGen2 | synchronous | N | optional |
| 50 | NanoBananaGenOR | synchronous | N | optional |
| 51 | NCBIDatasets | synchronous | N | niche (biology) |
| 52 | NovelAIGen | synchronous | N | optional |
| 53 | PaperReader | synchronous | **M** | §2.9 → S16.T3 |
| 54 | PowerShellExecutor | synchronous | **M** (defer) | §2.5 (Linux first) |
| 55 | ProjectAnalyst | synchronous | **S** | §3.15 (post-S11) |
| 56 | PubMedSearch | synchronous | N | niche (biology) |
| 57 | PyCameraCapture | synchronous | N | local desktop |
| 58 | PyScreenshot | synchronous | N | local desktop |
| 59 | QwenImageGen | synchronous | N | optional |
| 60 | RAGDiaryPlugin | hybridservice | **M** | §2.1 → S10 core |
| 61 | Randomness | synchronous | X | off-core |
| 62 | RiverTestPlugin | synchronous | X | test fixture |
| 63 | ScheduleBriefing | static | **S** | absorbed into S12 schedule UX |
| 64 | ScheduleManager | synchronous | **S** | §3.1 → S12.T1 |
| 65 | SciCalculator | synchronous | **S** | §3.2 → S11.T4 |
| 66 | SemanticGroupEditor | synchronous | N | absorb into §2.1 |
| 67 | SerpSearch | synchronous | **S** | §3.3 (recommend Tavily instead) |
| 68 | SnowBridge | hybridservice | X | openclaw-ecosystem |
| 69 | SunoGen | synchronous | N | optional |
| 70 | SVCardFinder | synchronous | N | very niche |
| 71 | SynapsePusher | (no manifest) | X | experimental |
| 72 | TagFolder | (no manifest) | N | optional |
| 73 | TarotDivination | synchronous | N | optional |
| 74 | TavilySearch | synchronous | **S** | §3.3 → S14.T1 |
| 75 | TencentCOSBackup | synchronous | **S** | §3.13 (post-S14) |
| 76 | ThoughtClusterManager | synchronous | **S** | §3.16 (post-S10) |
| 77 | ToolBoxFoldMemo | (no manifest) | X | experimental |
| 78 | UrlFetch | synchronous | **M** | §2.4 → S11.T2 |
| 79 | UserAuth | static | X | corlinman has stronger session auth |
| 80 | VCPEverything | synchronous | N | Windows Everything; optional |
| 81 | VCPForum | synchronous | X | openclaw-ecosystem |
| 82 | VCPForumAssistant | (no manifest) | X | openclaw-ecosystem |
| 83 | VCPForumLister | static | X | openclaw-ecosystem |
| 84 | VCPForumOnline | synchronous | X | openclaw-ecosystem |
| 85 | VCPForumOnlinePatrol | static | X | openclaw-ecosystem |
| 86 | VCPLog | service | X | covered by SSE logs |
| 87 | VCPTavern | hybridservice | **M** | §2.7 → S15.T1/T2 |
| 88 | VCPToolBridge | hybridservice | X | openclaw-ecosystem |
| 89 | VideoGenerator (Wan2.1) | asynchronous | N | optional |
| 90 | VSearch | synchronous | **S** | §3.14 (post-S16) |
| 91 | WeatherInfoNow | static | **S** | §3.10 → S14 (small) |
| 92 | WeatherReporter | static | **S** | §3.10 → S14 (small) |
| 93 | WebUIGen | synchronous | N | optional |
| 94 | WorkspaceInjector | messagePreprocessor | X | covered by placeholder renderer |
| 95 | XiaohongshuFetch | synchronous | **S** | §3.6 → S14 (optional) |
| 96 | ZImageGen | synchronous | N | optional |
| 97 | ZImageGen2 | synchronous | N | optional |
| 98 | ZImageTurboGen | synchronous | N | optional |
| 99 | DeepMemo (RAGDiaryPlugin internal) | — | **M** | §2.8 → S16.T2 |

Non-plugin subsystems:

| Subsystem | Bucket | Mapped to |
| --- | --- | --- |
| DailyNote (files + tagging + TagMemo + EPA + Pyramid + DeepMemo) | **M** | §2.1 → S10 + S16 |
| VCPTimedContacts | **M** | covered by S12.T3 |
| VCPChrome | **M** | §2.3 → S15 |
| SillyTavernSub | X | — |
| OpenWebUISub | X | — |
| AdminPanel | C | replaced by corlinman Next.js admin |
| vcp-installer-source | X | corlinman ships Docker |

Agent personas (`Agent/*.txt`):

| Persona | Bucket | Note |
| --- | --- | --- |
| Aemeath / DreamNova / Hornet / Nova / ThemeMaidCoco | X | fan-fiction IPs; ship generic templates instead |
| Metis | N | "cognitive archivist" — interesting template for a memory-admin agent, port as template |

---

_End of gap analysis._
