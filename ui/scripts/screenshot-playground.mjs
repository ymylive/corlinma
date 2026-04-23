#!/usr/bin/env node
// One-off screenshot helper for Phase 5d (Playground) retokening.
// Boots a Chromium page against the running dev server, captures dark + light
// theme variants, and writes them into worktree `screenshots/`.
import { chromium } from "@playwright/test";
import path from "node:path";
import fs from "node:fs";

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:3000";
const OUT_DIR = path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
  "..",
  "screenshots",
);
fs.mkdirSync(OUT_DIR, { recursive: true });

async function shoot(theme) {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 2,
    colorScheme: theme === "dark" ? "dark" : "light",
  });
  const page = await ctx.newPage();
  // The UI reads `?theme=…` via a layout hook in some projects, but the
  // Tidepool theme here also inherits from OS prefers-color-scheme. Setting
  // both belts-and-suspenders so whichever wins lines up with the request.
  const url = `${BASE_URL}/playground/protocol?theme=${theme}`;
  await page.goto(url, { waitUntil: "networkidle" });
  // Force the theme attribute in case there is a manual toggle active.
  await page.evaluate((t) => {
    document.documentElement.dataset.theme = t;
    document.documentElement.classList.toggle("dark", t === "dark");
  }, theme);
  await page.waitForTimeout(600);
  const out = path.join(OUT_DIR, `playground-protocol-${theme}.png`);
  await page.screenshot({ path: out, fullPage: true });
  await browser.close();
  console.log(`wrote ${out}`);
}

for (const theme of ["dark", "light"]) {
  await shoot(theme);
}
