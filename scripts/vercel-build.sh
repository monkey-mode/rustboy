#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=/dev/null
source "${HOME}/.cargo/env"

# ── Build WASM core ───────────────────────────────────────────────────────────
echo "Building WASM..."
cd core
wasm-pack build --target web --out-dir ../web/public/wasm --release
cd ..

# ── Build Next.js ─────────────────────────────────────────────────────────────
echo "Building Next.js..."
cd web
npm run build
cd ..