/**
 * Mock skill catalogue for the Skills Gallery (B2-FE3).
 *
 * Mirrors the eventual shape of `GET /admin/skills` (B2-BE1/BE5). Kept as a
 * plain module so the page can import it at build time without a network
 * round-trip; the real fetch wires in once the endpoint lands.
 */

export interface Skill {
  /** Stable id — matches the on-disk folder name under skills/. */
  name: string;
  /** One-line tagline shown on the card. */
  description: string;
  /** Emoji badge rendered in the card corner. Empty string falls back to an icon. */
  emoji: string;
  /** Tool identifiers this skill is allowed to invoke. Usually "plugin.tool". */
  allowed_tools: string[];
  /** Python/system packages the skill needs. */
  requires: string[];
  /** Install blurb rendered in the drawer. Short markdown ok. */
  install: string;
  /** Absolute path of the skill bundle on disk (server-side info). */
  source_path: string;
  /** Long-form markdown description rendered in the drawer body. */
  body_markdown: string;
}

export const MOCK_SKILLS: Skill[] = [
  {
    name: "web_search",
    description: "Query the live web via a pluggable search provider.",
    emoji: "🔎",
    allowed_tools: ["web_search.query", "web_search.fetch_page"],
    requires: ["httpx>=0.27"],
    install: "Drop an API key into `SERPER_API_KEY` or swap the provider in skill.toml.",
    source_path: "~/.corlinman/skills/web_search",
    body_markdown:
      "Performs live web search via Serper (default) or Tavily. Results come back as a ranked list of {title, url, snippet}. `fetch_page` follows a result URL and returns readable text.",
  },
  {
    name: "browser",
    description: "Headless Chromium for deterministic page interaction.",
    emoji: "🧭",
    allowed_tools: [
      "browser.navigate",
      "browser.click",
      "browser.type",
      "browser.screenshot",
      "browser.extract",
    ],
    requires: ["playwright>=1.49", "chromium"],
    install: "Run `playwright install chromium` once on the host.",
    source_path: "~/.corlinman/skills/browser",
    body_markdown:
      "Thin wrapper over Playwright. Each session pins to a fresh context so cookies and storage do not leak between invocations. `extract` returns a cleaned DOM tree suitable for LLM consumption.",
  },
  {
    name: "canvas",
    description: "Author and edit structured documents, code, and diagrams.",
    emoji: "🎨",
    allowed_tools: ["canvas.create", "canvas.patch", "canvas.render"],
    requires: [],
    install: "Bundled with the runtime — no install required.",
    source_path: "~/.corlinman/skills/canvas",
    body_markdown:
      "Opens a shared artifact buffer the agent can iterate on across turns. Supports markdown, HTML, SVG and mermaid. `patch` takes a unified diff.",
  },
  {
    name: "coding_agent",
    description: "Autonomous sub-agent for multi-step coding tasks.",
    emoji: "🧑‍💻",
    allowed_tools: [
      "coding_agent.spawn",
      "coding_agent.read_file",
      "coding_agent.write_file",
      "coding_agent.run",
    ],
    requires: ["git", "node>=20"],
    install:
      "Spawns in a sandboxed workdir under `skills/coding_agent/work/`. Cleanup is automatic on session end.",
    source_path: "~/.corlinman/skills/coding_agent",
    body_markdown:
      "A recursive sub-agent that can read, edit and execute code. Scoped to a workdir per invocation. Emits structured progress events so the parent agent can narrate.",
  },
  {
    name: "discord",
    description: "Post messages and react in Discord channels.",
    emoji: "💬",
    allowed_tools: ["discord.send_message", "discord.react", "discord.list_channels"],
    requires: ["discord.py>=2.4"],
    install: "Set `DISCORD_BOT_TOKEN` and invite the bot to your server.",
    source_path: "~/.corlinman/skills/discord",
    body_markdown:
      "Connects as a persistent bot client. `send_message` supports embeds and file attachments; `react` takes a message id and unicode emoji.",
  },
  {
    name: "gh_issues",
    description: "Read and triage GitHub issues and pull requests.",
    emoji: "🐙",
    allowed_tools: [
      "gh_issues.list",
      "gh_issues.get",
      "gh_issues.comment",
      "gh_issues.close",
    ],
    requires: ["gh>=2.40"],
    install: "Run `gh auth login` with a token scoped to the repos you care about.",
    source_path: "~/.corlinman/skills/gh_issues",
    body_markdown:
      "Thin wrapper around the `gh` CLI for auth consistency. Supports filters for label, author and state. Comments and closures are gated behind approvals by default.",
  },
  {
    name: "file_ops",
    description: "Safe local read/write across a pinned workspace.",
    emoji: "🗂️",
    allowed_tools: [
      "file_ops.read",
      "file_ops.write",
      "file_ops.list",
      "file_ops.glob",
    ],
    requires: [],
    install: "Workspace root defaults to `~/corlinman-workspace` — override via skill.toml.",
    source_path: "~/.corlinman/skills/file_ops",
    body_markdown:
      "Enforces a chroot-style allow-list: all paths are canonicalised and rejected if they escape the pinned workspace root.",
  },
  {
    name: "memory",
    description: "Vector-backed long-term memory for the agent.",
    emoji: "🧠",
    allowed_tools: ["memory.recall", "memory.store", "memory.forget"],
    requires: ["sqlite-vec"],
    install: "Creates `~/.corlinman/memory.db` on first use.",
    source_path: "~/.corlinman/skills/memory",
    body_markdown:
      "Stores embeddings alongside structured metadata in SQLite. `recall` does top-k cosine with optional tag filters; `forget` supports both id and semantic-query deletion.",
  },
  {
    name: "bear_notes",
    description: "Read and create notes in the Bear app on macOS.",
    emoji: "🐻",
    allowed_tools: ["bear_notes.search", "bear_notes.create", "bear_notes.open"],
    requires: ["macOS", "Bear app >= 2"],
    install: "Enable the Bear `x-callback-url` API in Bear → Preferences → General.",
    source_path: "~/.corlinman/skills/bear_notes",
    body_markdown:
      "Drives Bear via its x-callback-url interface. `search` returns note ids + titles; `create` appends or creates with optional tags.",
  },
  {
    name: "1password",
    description: "Fetch secrets from 1Password via the CLI.",
    emoji: "🔐",
    allowed_tools: ["1password.read_item", "1password.list_items"],
    requires: ["op>=2.24"],
    install: "Run `op signin` once; the skill inherits the session token from the host.",
    source_path: "~/.corlinman/skills/1password",
    body_markdown:
      "Read-only by default. Returns field values by id or ref. All invocations are approval-gated to prevent accidental exfiltration.",
  },
  {
    name: "gemini",
    description: "Use Google Gemini as a second-opinion model.",
    emoji: "✨",
    allowed_tools: ["gemini.chat", "gemini.embed"],
    requires: ["google-generativeai>=0.8"],
    install: "Set `GOOGLE_API_KEY`. Model defaults to `gemini-2.0-flash`.",
    source_path: "~/.corlinman/skills/gemini",
    body_markdown:
      "Routes a single-turn prompt to Gemini. Useful for cross-check / review workflows where you want a different model's take before committing.",
  },
  {
    name: "clawhub",
    description: "Internal knowledge-base lookup for Cyber Wizard.",
    emoji: "🦀",
    allowed_tools: ["clawhub.search", "clawhub.get_doc"],
    requires: ["httpx>=0.27", "CLAWHUB_TOKEN"],
    install: "Token issued by your Clawhub admin; export as `CLAWHUB_TOKEN`.",
    source_path: "~/.corlinman/skills/clawhub",
    body_markdown:
      "Semantic search against the Clawhub document corpus. Results include snippets with source refs so the agent can cite.",
  },
];
