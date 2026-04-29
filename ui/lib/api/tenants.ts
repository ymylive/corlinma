/**
 * Tenants admin API client (Phase 4 W1 4-1B).
 *
 * Wraps the operator-only `/admin/tenants` surface. The Rust gateway side
 * lands separately (corlinman-gateway::routes::admin::tenants); until then
 * the UI is mock-only via `ui/mock/server.ts`.
 *
 * Contract (mirrors the design doc — phase4-roadmap.md §4 row 4-1B):
 *
 *   GET  /admin/tenants
 *     → 200 { tenants: TenantRow[], allowed: string[] }
 *     → 401 if unauthenticated
 *     → 403 { error: "tenants_disabled" } when `[tenants].enabled = false`
 *           (legacy single-tenant mode); the page renders a banner instead.
 *
 *   POST /admin/tenants
 *     body: { slug, display_name?, admin_username, admin_password }
 *     → 201 { tenant_id }
 *     → 400 { error: "invalid_tenant_slug", reason }
 *     → 409 { error: "tenant_exists" }
 *
 * The slug regex `^[a-z][a-z0-9-]{0,62}$` is enforced server-side; the UI
 * only catches the obvious "empty" / "uppercase" cases for typing-time
 * UX polish, then surfaces the server's 400/409 responses inline.
 */

import { CorlinmanApiError, apiFetch } from "@/lib/api";

/** One row of the multi-tenant registry. Mirrors `tenants.sqlite`. */
export interface TenantRow {
  tenant_id: string;
  display_name: string;
  /** ISO-8601 created-at. */
  created_at: string;
}

export interface TenantsListResponse {
  tenants: TenantRow[];
  /** Slugs the current operator session is allowed to scope to. May be a
   *  subset of `tenants[]` for non-superuser operators (future). */
  allowed: string[];
}

export interface TenantCreateBody {
  slug: string;
  display_name?: string;
  admin_username: string;
  admin_password: string;
}

export interface TenantCreateResponse {
  tenant_id: string;
}

/** Reasons GET /admin/tenants can fail in a way the UI handles non-fatally. */
export type TenantsListState =
  | { kind: "ok"; data: TenantsListResponse }
  | { kind: "disabled" }
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string };

/**
 * Wrapper around GET /admin/tenants that maps the documented status codes
 * onto a tagged union. The page uses the tag to pick between rendering
 * the table, the "multi-tenant mode is off" banner, and the auth error.
 */
export async function fetchTenants(): Promise<TenantsListState> {
  try {
    const data = await apiFetch<TenantsListResponse>("/admin/tenants");
    return { kind: "ok", data };
  } catch (err) {
    if (err instanceof CorlinmanApiError) {
      if (err.status === 403 && /tenants_disabled/.test(err.message)) {
        return { kind: "disabled" };
      }
      if (err.status === 401) {
        return { kind: "unauthenticated" };
      }
      return { kind: "error", message: err.message };
    }
    return {
      kind: "error",
      message: err instanceof Error ? err.message : String(err),
    };
  }
}

/** POST /admin/tenants. Throws CorlinmanApiError on 400/409 — caller
 *  inspects `.status` + `.message` to drive inline form errors. */
export function createTenant(
  body: TenantCreateBody,
): Promise<TenantCreateResponse> {
  return apiFetch<TenantCreateResponse>("/admin/tenants", {
    method: "POST",
    body,
  });
}

/**
 * Slug regex — kept identical to the Rust validator in corlinman-tenant
 * so the UI's typing-time hint matches what the server will accept.
 * Exported for the create-tenant form's inline check.
 */
export const TENANT_SLUG_RE = /^[a-z][a-z0-9-]{0,62}$/;

export function isValidSlug(slug: string): boolean {
  return TENANT_SLUG_RE.test(slug);
}

/**
 * Build a URL for the current path with the `tenant=<slug>` query
 * parameter set (or removed when `slug` is null / matches the default
 * tenant). Pure — exported for unit testing the switcher's link logic
 * without driving the router.
 */
export function buildTenantHref(
  pathname: string,
  search: string,
  slug: string | null,
  defaultSlug = "default",
): string {
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  if (!slug || slug === defaultSlug) {
    params.delete("tenant");
  } else {
    params.set("tenant", slug);
  }
  const qs = params.toString();
  return qs ? `${pathname}?${qs}` : pathname;
}
