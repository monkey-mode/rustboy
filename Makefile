.PHONY: all build build-wasm build-web dev run clean fmt lint cargo-check test check install help

# Directories
CORE_DIR := core
WEB_DIR  := web
WASM_OUT := $(WEB_DIR)/public/wasm

## Default: build everything
all: build

## Build WASM + web
build: build-wasm build-web

## Compile Rust → WASM (release)
build-wasm:
	cd $(CORE_DIR) && wasm-pack build --target web --out-dir ../$(WASM_OUT) --release

## Compile Rust → WASM (debug, faster incremental builds)
build-wasm-dev:
	cd $(CORE_DIR) && wasm-pack build --target web --out-dir ../$(WASM_OUT) --dev

## Build Next.js for production
build-web:
	cd $(WEB_DIR) && npm run build

## Start Next.js dev server (builds WASM in dev mode first)
dev: build-wasm-dev
	cd $(WEB_DIR) && npm run dev

## Build optimized WASM then start dev server (best performance)
run: build-wasm
	cd $(WEB_DIR) && npm run dev

## Install all dependencies
install:
	cd $(WEB_DIR) && npm install

## Run Rust unit tests
test-core:
	cd $(CORE_DIR) && cargo test

## Run Next.js tests (if configured)
test-web:
	cd $(WEB_DIR) && npm test --if-present

## Run all tests
test: test-core test-web

## Format Rust code
fmt:
	cd $(CORE_DIR) && cargo fmt

## Check Rust formatting without writing
fmt-check:
	cd $(CORE_DIR) && cargo fmt -- --check

## Fast compile check for both native and WASM targets
cargo-check:
	cd $(CORE_DIR) && cargo check
	cd $(CORE_DIR) && cargo check --target wasm32-unknown-unknown

## Lint Rust code
lint:
	cd $(CORE_DIR) && cargo clippy -- -D warnings

## Type-check TypeScript
typecheck:
	cd $(WEB_DIR) && npx tsc --noEmit

## Run all checks (CI-friendly)
check: fmt-check cargo-check lint typecheck test-core

## Remove build artifacts
clean:
	cd $(CORE_DIR) && cargo clean
	rm -rf $(WASM_OUT)
	cd $(WEB_DIR) && rm -rf .next out

## Full clean including node_modules
clean-all: clean
	cd $(WEB_DIR) && rm -rf node_modules

help:
	@echo "Usage: make <target>"
	@echo ""
	@echo "  all            Build WASM + web (default)"
	@echo "  build          Same as all"
	@echo "  build-wasm     Build Rust core → WASM (release)"
	@echo "  build-wasm-dev Build Rust core → WASM (debug)"
	@echo "  build-web      Build Next.js production bundle"
	@echo "  dev            Build WASM (debug) then start Next.js dev server"
	@echo "  run            Build WASM (release + wasm-opt) then start dev server"
	@echo "  install        Install npm dependencies"
	@echo "  test           Run all tests"
	@echo "  test-core      Run Rust unit tests"
	@echo "  test-web       Run Next.js tests"
	@echo "  fmt            Format Rust code"
	@echo "  fmt-check      Check Rust formatting"
	@echo "  cargo-check    Fast compile check (no codegen)"
	@echo "  lint           Run cargo clippy"
	@echo "  typecheck      TypeScript type check"
	@echo "  check          Run all checks (CI)"
	@echo "  clean          Remove build artifacts"
	@echo "  clean-all      Remove build artifacts + node_modules"
