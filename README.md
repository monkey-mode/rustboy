# RustBoy

A multi-system emulator running in the browser — Game Boy (DMG) and NES, built with Rust + WebAssembly and Next.js.

## Systems

| System | CPU | Resolution | FPS |
|--------|-----|------------|-----|
| Game Boy (DMG) | Sharp LR35902 | 160×144 | 59.7 |
| NES (NTSC) | Ricoh 2A03 (6502) | 256×240 | 60.1 |

ROM format is auto-detected from the file header — no manual selection needed.

## Tech Stack

- **Emulator core** — Rust compiled to WebAssembly via `wasm-pack` + `wasm-bindgen`
- **Frontend** — Next.js 14 (App Router), React, TypeScript, Tailwind CSS

## Getting Started

### Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# WASM target
rustup target add wasm32-unknown-unknown

# wasm-pack
cargo install wasm-pack

# Node.js 18+
```

### Build & Run

```bash
# Install frontend dependencies
make install

# Build WASM (debug) + start dev server
make dev
```

Open [http://localhost:3000](http://localhost:3000), load a ROM, and play.

### Production Build

```bash
make build
```

## Project Structure

```
rustboy/
├── core/                  # Rust crate — all emulator backends
│   └── src/
│       ├── lib.rs         # wasm-bindgen exports, ROM auto-detection
│       ├── gb/            # Game Boy backend
│       │   ├── cpu.rs     # Sharp LR35902
│       │   ├── ppu.rs     # PPU — 160×144, 4-shade green palette
│       │   ├── apu.rs     # APU — square×2, wave, noise
│       │   ├── mmu.rs     # Memory bus + MBC0/MBC1
│       │   └── timer.rs   # DIV/TIMA/TMA/TAC
│       └── nes/           # NES backend
│           ├── cpu.rs     # Ricoh 2A03 — all 56 opcodes
│           ├── ppu.rs     # PPU — Loopy scroll, sprites, 64-color palette
│           ├── apu.rs     # APU — Pulse×2, Triangle, Noise, DMC
│           ├── bus.rs     # Memory bus, OAM DMA, joypad
│           └── cartridge.rs  # iNES parser, Mapper 0/1/2/3/4
└── web/                   # Next.js app
    ├── app/
    ├── components/
    │   ├── Emulator.tsx   # ROM loader, frame loop, audio scheduling
    │   ├── Screen.tsx     # Canvas renderer (pixelated, variable resolution)
    │   └── Controls.tsx   # On-screen buttons + keyboard input
    └── hooks/
        └── useEmulator.ts # WASM lifecycle hook
```

## Controls

| Action | Keys |
|--------|------|
| D-pad | Arrow keys / WASD |
| A | Z / J |
| B | X / K |
| Select | Shift / I |
| Start | Enter / L |

On-screen buttons are also available for touch/mouse.

## Make Targets

```bash
make run            # Build optimized WASM + start dev server (best performance)
make dev            # Build WASM (debug) + start Next.js dev server
make build          # Release build of WASM + Next.js
make build-wasm     # Rust → WASM (release) only
make build-wasm-dev # Rust → WASM (debug) only
make install        # npm install in web/
make cargo-check    # Fast compile check (native + wasm32)
make lint           # cargo clippy
make fmt            # cargo fmt
make test           # Run all tests
make check          # Full CI check (fmt + check + lint + typecheck + tests)
make clean          # Remove build artifacts
```

## Supported Mappers (NES)

| Mapper | Name | Examples |
|--------|------|---------|
| 0 | NROM | Donkey Kong, Super Mario Bros. |
| 1 | MMC1 | Mega Man 2, Legend of Zelda |
| 2 | UxROM | Contra, Castlevania |
| 3 | CNROM | Excitebike |
| 4 | MMC3 | Contra Force, Super Mario Bros. 3, Mega Man 3-6 |