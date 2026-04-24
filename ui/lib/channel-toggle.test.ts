import { describe, expect, it } from "vitest";

import { patchChannelEnabled, readChannelEnabled } from "./api";

describe("channel enable TOML patcher", () => {
  const base = `[server]
port = 6005

[channels]

[channels.qq]
enabled = false
ws_url = "ws://napcat:3001"
napcat_access_token = { value = "fef5d17d6e24" }

[rag]
top_k = 5
`;

  it("reads enabled=false for qq from config with qq section present", () => {
    expect(readChannelEnabled(base, "qq")).toBe(false);
  });

  it("reads enabled=false for telegram when section is missing", () => {
    expect(readChannelEnabled(base, "telegram")).toBe(false);
  });

  it("flips qq.enabled without disturbing surrounding TOML", () => {
    const next = patchChannelEnabled(base, "qq", true);
    expect(next).toContain("[channels.qq]\nenabled = true");
    // Fields after `enabled` stay intact
    expect(next).toContain('ws_url = "ws://napcat:3001"');
    expect(next).toContain('napcat_access_token = { value = "fef5d17d6e24" }');
    // Next section still present
    expect(next).toContain("\n[rag]\ntop_k = 5");
    // Round-trip: reading back returns the new value
    expect(readChannelEnabled(next, "qq")).toBe(true);
  });

  it("appends [channels.telegram] section when absent", () => {
    const next = patchChannelEnabled(base, "telegram", true);
    expect(next).toContain("[channels.telegram]\nenabled = true");
    // Did not touch qq
    expect(next).toMatch(/\[channels\.qq\]\s*\nenabled = false/);
    expect(readChannelEnabled(next, "telegram")).toBe(true);
  });

  it("preserves a trailing-comment on the enabled line", () => {
    const withComment = base.replace(
      "enabled = false",
      "enabled = false  # temporarily off",
    );
    const next = patchChannelEnabled(withComment, "qq", true);
    // The patcher only replaces `true|false`, so the trailing comment stays.
    expect(next).toContain("enabled = true  # temporarily off");
  });

  it("adds enabled key to an existing section that lacks one", () => {
    const noEnabled = `[channels.qq]
ws_url = "ws://napcat:3001"
`;
    const next = patchChannelEnabled(noEnabled, "qq", true);
    expect(next).toMatch(/\[channels\.qq\]\s*\nenabled = true/);
    expect(next).toContain('ws_url = "ws://napcat:3001"');
    expect(readChannelEnabled(next, "qq")).toBe(true);
  });

  it("flipping twice round-trips to the original enabled value", () => {
    const on = patchChannelEnabled(base, "qq", true);
    const off = patchChannelEnabled(on, "qq", false);
    expect(readChannelEnabled(off, "qq")).toBe(false);
  });
});
