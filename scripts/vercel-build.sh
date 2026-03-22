#!/usr/bin/env bash
set -euo pipefail

# ── Rust toolchain ────────────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  echo "Installing Rust..."
  curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
fi
# shellcheck source=/dev/null
source "${HOME}/.cargo/env"

rustup target add wasm32-unknown-unknown

# ── wasm-pack ─────────────────────────────────────────────────────────────────
if ! command -v wasm-pack &>/dev/null; then
  echo "Installing wasm-pack..."
  curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# ── Build WASM core ───────────────────────────────────────────────────────────
echo "Building WASM..."
cd core
wasm-pack build --target web --out-dir ../web/public/wasm --release
cd ..

# ── Build Next.js ─────────────────────────────────────────────────────────────
echo "Building Next.js..."
cd web
npm run build
