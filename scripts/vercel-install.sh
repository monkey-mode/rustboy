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

# ── Node.js and npm ───────────────────────────────────────────────────────────
# Vercel provides Node.js and npm, so we can skip installation if they are already available.

# ── Install Next.js dependencies ─────────────────────────────────────────────
echo "Installing Next.js dependencies..."
cd web
npm install