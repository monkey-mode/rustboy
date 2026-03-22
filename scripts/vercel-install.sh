#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ── Rust toolchain ────────────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  echo "Installing Rust..."
  curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable
fi
# shellcheck source=/dev/null
[ -f "${HOME}/.cargo/env" ] && source "${HOME}/.cargo/env"

rustup target add wasm32-unknown-unknown

# ── wasm-pack ─────────────────────────────────────────────────────────────────
if ! command -v wasm-pack &>/dev/null; then
  echo "Installing wasm-pack..."
  curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# ── Install Next.js dependencies ─────────────────────────────────────────────
echo "Installing Next.js dependencies..."
cd web && npm install
