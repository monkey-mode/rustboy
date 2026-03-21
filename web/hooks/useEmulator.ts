"use client";

import { useEffect, useState } from "react";

// Type matching the wasm-bindgen generated Emulator class.
export interface WasmEmulator {
  new(rom: Uint8Array): WasmEmulator;
  step_frame(): void;
  frame_buffer(): number; // pointer into WASM linear memory
  set_joypad(button: number, pressed: boolean): void;
  audio_buffer(): Float32Array;
  free(): void;
}

export interface WasmModule {
  Emulator: {
    new(rom: Uint8Array): WasmEmulator;
  };
  memory: WebAssembly.Memory;
}

interface UseEmulatorResult {
  wasmModule: WasmModule | null;
  isLoaded: boolean;
  error: string | null;
}

/**
 * Dynamically loads the rustboy-core WASM module.
 *
 * The module is expected to be built with wasm-pack into
 * `public/wasm/rustboy_core.js` (the JS glue) and
 * `public/wasm/rustboy_core_bg.wasm`.
 *
 * Run:
 *   wasm-pack build ../core --target web --out-dir ../web/public/wasm
 */
export function useEmulator(): UseEmulatorResult {
  const [wasmModule, setWasmModule] = useState<WasmModule | null>(null);
  const [isLoaded, setIsLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadWasm() {
      try {
        // Dynamic import of the wasm-pack generated JS glue.
        // The module lives at /public/wasm/ which Next.js serves from /.
        const wasmJs = await import(
          /* webpackIgnore: true */ "/wasm/rustboy_core.js"
        ) as WasmModule & { default: (input?: string) => Promise<void> };

        // Initialize the WASM binary.
        await wasmJs.default("/wasm/rustboy_core_bg.wasm");

        if (!cancelled) {
          setWasmModule(wasmJs);
          setIsLoaded(true);
        }
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          setError(`Failed to load WASM module: ${message}`);
        }
      }
    }

    loadWasm();

    return () => {
      cancelled = true;
    };
  }, []);

  return { wasmModule, isLoaded, error };
}
