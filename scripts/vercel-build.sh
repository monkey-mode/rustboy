#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# shellcheck source=/dev/null
[ -f "${HOME}/.cargo/env" ] && source "${HOME}/.cargo/env"

# ── Build WASM core ───────────────────────────────────────────────────────────
echo "Building WASM..."
cd core
wasm-pack build --target web --out-dir ../web/public/wasm --release
cd "$REPO_ROOT"

# ── Build Next.js ─────────────────────────────────────────────────────────────
echo "Building Next.js..."
cd web && npm run build
