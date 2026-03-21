"use client";

import {
  useRef,
  useState,
  useCallback,
  useEffect,
  ChangeEvent,
} from "react";
import Screen from "./Screen";
import Controls, { ButtonIndex } from "./Controls";
import { useEmulator, WasmEmulator, WasmModule } from "@/hooks/useEmulator";


export default function Emulator() {
  const { wasmModule, isLoaded, error } = useEmulator();

  const emulatorRef  = useRef<WasmEmulator | null>(null);
  const rafRef       = useRef<number | null>(null);
  const audioCtxRef  = useRef<AudioContext | null>(null);
  const nextAudioRef = useRef<number>(0);

  const [frameBuffer, setFrameBuffer] = useState<Uint8Array | null>(null);
  const [screenW,     setScreenW]     = useState(160);
  const [screenH,     setScreenH]     = useState(144);
  const [romName,     setRomName]     = useState<string | null>(null);
  const [running,     setRunning]     = useState(false);
  const [loadError,   setLoadError]   = useState<string | null>(null);

  // -----------------------------------------------------------------------
  // Audio helpers
  // -----------------------------------------------------------------------
  function ensureAudioContext() {
    if (!audioCtxRef.current) {
      audioCtxRef.current = new AudioContext({ sampleRate: 44100 });
      nextAudioRef.current = audioCtxRef.current.currentTime;
    }
    return audioCtxRef.current;
  }

  function scheduleAudio(samples: Float32Array) {
    if (samples.length === 0) return;
    const ctx = ensureAudioContext();
    const buf = ctx.createBuffer(1, samples.length, 44100);
    buf.copyToChannel(new Float32Array(samples.buffer as ArrayBuffer, samples.byteOffset, samples.length), 0);
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.connect(ctx.destination);

    const now = ctx.currentTime;
    const LOOKAHEAD = 0.02;  // 20ms minimum ahead
    const MAX_AHEAD = 0.05;  // 50ms maximum — drop excess to avoid drift

    if (nextAudioRef.current < now + LOOKAHEAD) {
      nextAudioRef.current = now + LOOKAHEAD;
    } else if (nextAudioRef.current > now + MAX_AHEAD) {
      // Queue drifted too far ahead — snap back to prevent growing lag
      nextAudioRef.current = now + LOOKAHEAD;
    }
    // NES runs at 60.0988 fps; rAF targets 60.0 fps → audio is 0.16% fast.
    // Compensate by slowing playback rate: 60.0 / 60.0988 ≈ 0.9984
    src.playbackRate.value = 60.0 / 60.0988;
    src.start(nextAudioRef.current);
    nextAudioRef.current += buf.duration / src.playbackRate.value;
  }

  // -----------------------------------------------------------------------
  // Frame loop
  // -----------------------------------------------------------------------
  const frameLoop = useCallback(() => {
    const emu = emulatorRef.current;
    if (!emu) return;

    emu.step_frame();

    setFrameBuffer(emu.frame_buffer());

    // Audio
    const rawSamples = emu.audio_buffer();
    scheduleAudio(rawSamples);

    rafRef.current = requestAnimationFrame(frameLoop);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wasmModule]);

  // -----------------------------------------------------------------------
  // ROM loading
  // -----------------------------------------------------------------------
  const handleRomLoad = useCallback(
    async (e: ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file || !wasmModule) return;

      setLoadError(null);

      try {
        const buffer = await file.arrayBuffer();
        const romBytes = new Uint8Array(buffer);

        // Stop existing emulator
        if (rafRef.current !== null) {
          cancelAnimationFrame(rafRef.current);
          rafRef.current = null;
        }
        if (emulatorRef.current) {
          emulatorRef.current.free();
          emulatorRef.current = null;
        }
        if (audioCtxRef.current) {
          await audioCtxRef.current.close();
          audioCtxRef.current = null;
        }
        nextAudioRef.current = 0;

        const emu = new (wasmModule as unknown as WasmModule).Emulator(romBytes);
        emulatorRef.current = emu;
        setScreenW(emu.frame_width());
        setScreenH(emu.frame_height());
        setRomName(file.name);
        setRunning(true);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setLoadError(`Failed to load ROM: ${msg}`);
      }
    },
    [wasmModule]
  );

  // Start / stop frame loop when `running` changes
  useEffect(() => {
    if (running && emulatorRef.current) {
      rafRef.current = requestAnimationFrame(frameLoop);
    } else if (!running && rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    return () => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [running, frameLoop]);

  // -----------------------------------------------------------------------
  // Joypad
  // -----------------------------------------------------------------------
  const handleButton = useCallback(
    (button: ButtonIndex, pressed: boolean) => {
      emulatorRef.current?.set_joypad(button, pressed);
    },
    []
  );

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------
  if (error) {
    return (
      <div className="text-red-400 text-sm p-4 border border-red-700 rounded">
        {error}
      </div>
    );
  }

  if (!isLoaded) {
    return (
      <div className="text-gray-400 text-sm animate-pulse">
        Loading WASM module…
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center gap-4">
      {/* ROM picker */}
      <div className="flex flex-col items-center gap-2">
        <label
          htmlFor="rom-input"
          className="cursor-pointer px-4 py-2 bg-green-700 hover:bg-green-600 rounded text-sm font-semibold transition-colors"
        >
          {romName ? `ROM: ${romName}` : "Load ROM (.gb / .nes)"}
        </label>
        <input
          id="rom-input"
          type="file"
          accept=".gb,.gbc,.nes"
          className="hidden"
          onChange={handleRomLoad}
        />
        {loadError && (
          <p className="text-red-400 text-xs">{loadError}</p>
        )}
      </div>

      {/* Screen */}
      <Screen frameBuffer={frameBuffer} width={screenW} height={screenH} />

      {/* Controls */}
      <Controls onButton={handleButton} />

      {/* Keyboard hints */}
      <p className="text-gray-600 text-xs mt-2">
        Keyboard: Arrows / WASD = D-pad · Z/J = A · X/K = B · I = Select · L/Enter = Start · Shift = Select
      </p>
    </div>
  );
}
