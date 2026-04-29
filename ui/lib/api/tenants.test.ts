/**
 * Pure-function tests for `lib/api/tenants.ts` — covers the URL-builder
 * the tenant switcher uses, and the slug regex export.
 */

import { describe, expect, it } from "vitest";

import { buildTenantHref, isValidSlug, TENANT_SLUG_RE } from "./tenants";

describe("buildTenantHref", () => {
  it("appends ?tenant=<slug> when none was set", () => {
    expect(buildTenantHref("/plugins", "", "acme")).toBe(
      "/plugins?tenant=acme",
    );
  });

  it("preserves existing query params and overwrites tenant", () => {
    expect(
      buildTenantHref("/plugins", "?filter=loaded&tenant=old", "acme"),
    ).toBe("/plugins?filter=loaded&tenant=acme");
  });

  it("strips ?tenant when picking the default slug", () => {
    expect(buildTenantHref("/plugins", "?tenant=acme", "default")).toBe(
      "/plugins",
    );
  });

  it("strips ?tenant when slug is null", () => {
    expect(buildTenantHref("/plugins", "?tenant=acme", null)).toBe(
      "/plugins",
    );
  });

  it("preserves other params when stripping tenant", () => {
    expect(
      buildTenantHref("/plugins", "?filter=loaded&tenant=acme", "default"),
    ).toBe("/plugins?filter=loaded");
  });

  it("accepts a search string without a leading question mark", () => {
    expect(buildTenantHref("/plugins", "filter=loaded", "acme")).toBe(
      "/plugins?filter=loaded&tenant=acme",
    );
  });

  it("honours a custom defaultSlug", () => {
    // When the default tenant in this deployment is `main`, picking it
    // should also strip the param.
    expect(
      buildTenantHref("/plugins", "?tenant=main", "main", "main"),
    ).toBe("/plugins");
  });
});

describe("TENANT_SLUG_RE / isValidSlug", () => {
  it.each([
    ["acme", true],
    ["acme-co", true],
    ["a", true],
    ["a1-b2", true],
    // 63 chars is the max allowed per the Rust regex (1 + 62).
    ["a" + "b".repeat(62), true],
    ["", false],
    ["1acme", false],
    ["-acme", false],
    ["ACME", false],
    ["acme!", false],
    ["a" + "b".repeat(63), false],
  ])("%s → %s", (input, expected) => {
    expect(isValidSlug(input)).toBe(expected);
    expect(TENANT_SLUG_RE.test(input)).toBe(expected);
  });
});
