/**
 * Build the inert HTML document used as the Canvas page's iframe `srcDoc`.
 *
 * Kept out of the React render path so re-renders don't re-serialise the
 * template. Colours match the warm-orange Tidepool palette (light + dark
 * modes via `prefers-color-scheme`).
 */
export function buildPlaceholderHtml(sessionId: string): string {
  const safeId = String(sessionId).replace(/[<>&"]/g, "");
  return `<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>Canvas placeholder</title>
<style>
  :root { color-scheme: light dark; }
  html, body { margin: 0; height: 100%; }
  body {
    display: flex; align-items: center; justify-content: center;
    font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
    background:
      radial-gradient(900px 300px at 20% 0%, rgba(230,120,60,0.12), transparent 60%),
      radial-gradient(700px 300px at 80% 100%, rgba(250,180,110,0.10), transparent 60%),
      #120a07;
    color: #e8d9ce;
  }
  .card { text-align: center; letter-spacing: 0.02em; }
  .title { font-size: 12px; color: #a88970; text-transform: uppercase; letter-spacing: 0.18em; }
  .session { margin-top: 10px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 15px; color: #e8d9ce; }
  @media (prefers-color-scheme: light) {
    body {
      background:
        radial-gradient(900px 300px at 20% 0%, rgba(230,120,60,0.10), transparent 60%),
        radial-gradient(700px 300px at 80% 100%, rgba(250,180,110,0.10), transparent 60%),
        #fff6ee;
      color: #3b2a20;
    }
    .title { color: #8a6a55; }
    .session { color: #3b2a20; }
  }
</style>
</head>
<body>
<div class="card">
  <div class="title">Canvas placeholder</div>
  <div class="session">Session: ${safeId}</div>
</div>
</body>
</html>`;
}

/** Human-readable byte count. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}KB`;
  return `${(n / (1024 * 1024)).toFixed(1)}MB`;
}
