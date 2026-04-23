#!/usr/bin/env bash
# Full local verification matrix for corlinman.
#
# For local dev — CI runs each step as its own job. Run from repo root:
#   bash scripts/e2e/run.sh
#
# Mirrors `docs/release/v0.3.0-checklist.md` §Tests.

set -euo pipefail
cd "$(dirname "$0")/../.."

step() { printf "\n\033[1;34m[%s]\033[0m %s\n" "$1" "$2"; }

step "1/6" "cargo fmt --all --check"
cargo fmt --all --check

step "2/6" "cargo clippy --workspace --all-targets"
cargo clippy --workspace --all-targets -- -D warnings

step "3/6" "cargo test --workspace"
cargo test --workspace --no-fail-fast

step "4/6" "uv run pytest python/packages"
uv run pytest python/packages/

step "5/6" "pnpm lint + typecheck + test (ui/)"
( cd ui && pnpm install --frozen-lockfile && pnpm lint && pnpm typecheck && pnpm test --run )

step "6/6" "Batch 5 integration regression"
cargo test -p corlinman-integration-tests --test b5_final

printf "\n\033[1;32mAll gates green.\033[0m\n"
