/**
 * Identity admin API client (Phase 4 W2 B2 iter 7).
 *
 * Mirrors the Rust HTTP routes at
 * `rust/crates/corlinman-gateway/src/routes/admin/identity.rs`:
 *
 *   GET /admin/identity?limit=&offset=
 *     → 200 { users: UserSummary[] }
 *     → 503 { error: "identity_disabled" }
 *
 *   GET /admin/identity/:user_id
 *     → 200 { user_id, aliases: ChannelAlias[] }
 *     → 404 { error: "not_found", user_id }
 *     → 503 { error: "identity_disabled" }
 *
 *   POST /admin/identity/:user_id/issue-phrase
 *     body { channel: string, channel_user_id: string }
 *     → 201 { phrase, expires_at, user_id }
 *     → 400 { error: "invalid_input" }
 *     → 503
 *
 *   POST /admin/identity/merge
 *     body { into_user_id, from_user_id, decided_by }
 *     → 200 { surviving_user_id }
 *     → 400 | 404 | 503
 *
 * Tagged result types so consumers can branch on 503/404 without
 * pattern-matching exception messages.
 */

import { CorlinmanApiError, apiFetch } from "@/lib/api";

/* ------------------------------------------------------------------ */
/*                           Public types                             */
/* ------------------------------------------------------------------ */

/** Row in `GET /admin/identity`. Mirrors Rust `UserSummary`. */
export interface UserSummary {
  user_id: string;
  display_name: string | null;
  alias_count: number;
}

export type BindingKind = "auto" | "verified" | "operator";

/** One row in the alias table for a single user. */
export interface ChannelAlias {
  channel: string;
  channel_user_id: string;
  user_id: string;
  binding_kind: BindingKind;
  /** RFC-3339 / ISO-8601 string. */
  created_at: string;
}

export interface UserDetailResponse {
  user_id: string;
  aliases: ChannelAlias[];
}

export interface IssuePhraseResponse {
  phrase: string;
  /** RFC-3339; the operator echoes the phrase + this deadline to the user. */
  expires_at: string;
  user_id: string;
}

export interface MergeResponse {
  surviving_user_id: string;
}

/* ------------------------------------------------------------------ */
/*                       Tagged result types                          */
/* ------------------------------------------------------------------ */

export type IdentityListResult =
  | { kind: "ok"; users: UserSummary[] }
  | { kind: "disabled" };

export type IdentityDetailResult =
  | { kind: "ok"; detail: UserDetailResponse }
  | { kind: "not_found"; user_id: string }
  | { kind: "disabled" };

export type IssuePhraseResult =
  | { kind: "ok"; response: IssuePhraseResponse }
  | { kind: "invalid_input"; message: string }
  | { kind: "disabled" };

export type MergeResult =
  | { kind: "ok"; response: MergeResponse }
  | { kind: "not_found"; user_id: string }
  | { kind: "invalid_input"; message: string }
  | { kind: "disabled" };

/* ------------------------------------------------------------------ */
/*                          URL builders                              */
/* ------------------------------------------------------------------ */

export const IDENTITY_LIST_PATH = "/admin/identity";

export function identityDetailPath(userId: string): string {
  return `/admin/identity/${encodeURIComponent(userId)}`;
}

export function identityIssuePhrasePath(userId: string): string {
  return `/admin/identity/${encodeURIComponent(userId)}/issue-phrase`;
}

export const IDENTITY_MERGE_PATH = "/admin/identity/merge";

/* ------------------------------------------------------------------ */
/*                          Error helpers                             */
/* ------------------------------------------------------------------ */

function is503(err: unknown): boolean {
  return err instanceof CorlinmanApiError && err.status === 503;
}

function is404(err: unknown): boolean {
  return err instanceof CorlinmanApiError && err.status === 404;
}

function is400(err: unknown): boolean {
  return err instanceof CorlinmanApiError && err.status === 400;
}

/* ------------------------------------------------------------------ */
/*                            Public fetches                          */
/* ------------------------------------------------------------------ */

export async function fetchIdentityList(
  opts: { limit?: number; offset?: number } = {},
): Promise<IdentityListResult> {
  const params = new URLSearchParams();
  if (typeof opts.limit === "number") params.set("limit", String(opts.limit));
  if (typeof opts.offset === "number") params.set("offset", String(opts.offset));
  const qs = params.toString();
  try {
    const res = await apiFetch<{ users: UserSummary[] }>(
      qs ? `${IDENTITY_LIST_PATH}?${qs}` : IDENTITY_LIST_PATH,
    );
    return { kind: "ok", users: res.users ?? [] };
  } catch (err) {
    if (is503(err)) return { kind: "disabled" };
    throw err;
  }
}

export async function fetchIdentityDetail(
  userId: string,
): Promise<IdentityDetailResult> {
  try {
    const detail = await apiFetch<UserDetailResponse>(identityDetailPath(userId));
    return { kind: "ok", detail };
  } catch (err) {
    if (is404(err)) return { kind: "not_found", user_id: userId };
    if (is503(err)) return { kind: "disabled" };
    throw err;
  }
}

export async function issueIdentityPhrase(
  userId: string,
  body: { channel: string; channel_user_id: string },
): Promise<IssuePhraseResult> {
  try {
    const response = await apiFetch<IssuePhraseResponse>(
      identityIssuePhrasePath(userId),
      { method: "POST", body },
    );
    return { kind: "ok", response };
  } catch (err) {
    if (is503(err)) return { kind: "disabled" };
    if (is400(err)) {
      return {
        kind: "invalid_input",
        message: err instanceof CorlinmanApiError ? err.message : "invalid input",
      };
    }
    throw err;
  }
}

export async function mergeIdentities(body: {
  into_user_id: string;
  from_user_id: string;
  decided_by: string;
}): Promise<MergeResult> {
  try {
    const response = await apiFetch<MergeResponse>(IDENTITY_MERGE_PATH, {
      method: "POST",
      body,
    });
    return { kind: "ok", response };
  } catch (err) {
    if (is503(err)) return { kind: "disabled" };
    if (is404(err)) {
      return { kind: "not_found", user_id: body.from_user_id };
    }
    if (is400(err)) {
      return {
        kind: "invalid_input",
        message: err instanceof CorlinmanApiError ? err.message : "invalid input",
      };
    }
    throw err;
  }
}
