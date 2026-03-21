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

  const emulatorRef    = useRef<WasmEmulator | null>(null);
  const rafRef         = useRef<number | null>(null);
  const audioCtxRef    = useRef<AudioContext | null>(null);
  const nextAudioRef   = useRef<number>(0);
  const lastFrameRef   = useRef<number>(0);

  const [frameBuffer, setFrameBuffer] = useState<Uint8Array | null>(null);
  const [screenW,     setScreenW]     = useState(160);
  const [screenH,     setScreenH]     = useState(144);
  const [romName,     setRomName]     = useState<string | null>(null);
  const [system,      setSystem]      = useState<"GB" | "NES" | null>(null);
  const [running,     setRunning]     = useState(false);
  const [loadError,   setLoadError]   = useState<string | null>(null);
  const [saveSlots,   setSaveSlots]   = useState<(Uint8Array | null)[]>([null, null, null]);

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
  // GB: 59.7 fps (~16.75 ms/frame), NES: 60.0988 fps (~16.63 ms/frame)
  // Use 16.67 ms (60 fps) as a safe target for both — prevents running at
  // 2× speed on 120 Hz / 144 Hz displays.
  const FRAME_MS = 1000 / 60;

  const frameLoop = useCallback((timestamp: number) => {
    const emu = emulatorRef.current;
    if (!emu) return;

    const elapsed = timestamp - lastFrameRef.current;
    if (elapsed >= FRAME_MS) {
      lastFrameRef.current = timestamp - (elapsed % FRAME_MS);

      emu.step_frame();
      setFrameBuffer(emu.frame_buffer());

      const rawSamples = emu.audio_buffer();
      scheduleAudio(rawSamples);
    }

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

      // Stop the current loop immediately (don't wait for effect).
      // Also set running=false so the effect won't restart it.
      setRunning(false);
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }

      try {
        const buffer = await file.arrayBuffer();
        const romBytes = new Uint8Array(buffer);

        if (emulatorRef.current) {
          emulatorRef.current.free();
          emulatorRef.current = null;
        }
        if (audioCtxRef.current) {
          await audioCtxRef.current.close();
          audioCtxRef.current = null;
        }
        nextAudioRef.current = 0;
        lastFrameRef.current = 0; // reset frame timer so first frame fires immediately

        const emu = new (wasmModule as unknown as WasmModule).Emulator(romBytes);
        emulatorRef.current = emu;
        const w = emu.frame_width();
        const h = emu.frame_height();
        setScreenW(w);
        setScreenH(h);
        setSystem(w === 256 ? "NES" : "GB");
        setRomName(file.name);
        setRunning(true); // running went false → true, so the effect will fire
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
  // Save / Load state helpers
  // -----------------------------------------------------------------------
  const lsKey = useCallback(
    (slot: number) => `rustboy_save_${romName ?? "unknown"}_slot${slot}`,
    [romName]
  );

  // Load persisted slots from localStorage whenever the ROM name changes.
  useEffect(() => {
    if (!romName) return;
    setSaveSlots((prev) => {
      const next = [...prev] as (Uint8Array | null)[];
      for (let i = 0; i < 3; i++) {
        try {
          const raw = localStorage.getItem(lsKey(i));
          if (raw) {
            const decoded = Uint8Array.from(atob(raw), (c) => c.charCodeAt(0));
            next[i] = decoded;
          }
        } catch {
          // ignore corrupt entries
        }
      }
      return next;
    });
  }, [romName, lsKey]);

  const handleSave = useCallback(
    (slot: number) => {
      const emu = emulatorRef.current;
      if (!emu) return;
      const data = emu.save_state();
      setSaveSlots((prev) => {
        const next = [...prev] as (Uint8Array | null)[];
        next[slot] = data;
        return next;
      });
      // Persist to localStorage as base64
      try {
        const b64 = btoa(Array.from(data, (b) => String.fromCharCode(b)).join(""));
        localStorage.setItem(lsKey(slot), b64);
      } catch {
        // quota exceeded or unavailable — silently skip
      }
    },
    [lsKey]
  );

  const handleLoad = useCallback(
    (slot: number, slots: (Uint8Array | null)[]) => {
      const emu = emulatorRef.current;
      const data = slots[slot];
      if (!emu || !data) return;
      emu.load_state(data);
      setFrameBuffer(emu.frame_buffer());
    },
    []
  );

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
      <div className="text-red-400 text-sm px-4 py-3 rounded-lg border border-red-900 bg-red-950/40">
        {error}
      </div>
    );
  }

  if (!isLoaded) {
    return (
      <div className="flex items-center gap-2 text-gray-500 text-sm">
        <span className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
        Loading WebAssembly…
      </div>
    );
  }

  const systemColor = system === "NES"
    ? "text-red-400 border-red-800 bg-red-950/40"
    : "text-green-400 border-green-800 bg-green-950/40";

  return (
    <div className="flex items-start gap-5">

      {/* ── Left panel ── */}
      <aside className="flex flex-col gap-4 w-40 pt-1">
        {/* Load ROM */}
        <div className="flex flex-col gap-1.5">
          <label
            htmlFor="rom-input"
            className="cursor-pointer text-center px-3 py-2 rounded-lg border border-green-800
              bg-green-950/40 hover:bg-green-900/50 text-green-400 text-xs font-semibold
              tracking-wide transition-colors"
          >
            {romName ? "Change ROM" : "Load ROM"}
          </label>
          <input id="rom-input" type="file" accept=".gb,.gbc,.nes"
            className="hidden" onChange={handleRomLoad} />
          {loadError && <p className="text-red-400 text-[10px] leading-tight">{loadError}</p>}
        </div>

        {/* Keyboard reference */}
        <div className="rounded-lg border border-gray-800 bg-gray-900/60 p-3 flex flex-col gap-1.5">
          <p className="text-gray-500 text-[10px] uppercase tracking-widest mb-1">Keyboard</p>
          {[
            ["WASD", "D-pad"],
            ["J", "A"],
            ["K", "B"],
            ["I", "Select"],
            ["L", "Start"],
          ].map(([key, action]) => (
            <div key={key} className="flex justify-between items-center">
              <span className="text-gray-400 text-[10px] font-mono">{key}</span>
              <span className="text-gray-600 text-[10px]">{action}</span>
            </div>
          ))}
        </div>
      </aside>

      {/* ── Center: screen + controller bar ── */}
      <div className="flex flex-col">
        <Screen frameBuffer={frameBuffer} width={screenW} height={screenH} />
        <Controls onButton={handleButton} />
      </div>

      {/* ── Right panel ── */}
      <aside className="flex flex-col gap-4 w-40 pt-1">
        {/* System + ROM info */}
        <div className="rounded-lg border border-gray-800 bg-gray-900/60 p-3 flex flex-col gap-2">
          {system ? (
            <span className={`self-start text-xs font-mono font-bold px-2 py-0.5 rounded border ${systemColor}`}>
              {system}
            </span>
          ) : (
            <span className="text-gray-600 text-[10px]">No ROM loaded</span>
          )}
          {romName && (
            <p className="text-gray-500 text-[10px] font-mono break-all leading-relaxed">
              {romName}
            </p>
          )}
          {system && (
            <p className="text-gray-700 text-[10px] leading-relaxed mt-1">
              {system === "NES"
                ? "256×240 · 60 fps\nRicoh 2A03"
                : "160×144 · 60 fps\nSharp LR35902"}
            </p>
          )}
        </div>

        {/* Save slots */}
        <div className="rounded-lg border border-gray-800 bg-gray-900/60 p-3 flex flex-col gap-2">
          <p className="text-gray-500 text-[10px] uppercase tracking-widest">Save States</p>
          {([0, 1, 2] as const).map((slot) => (
            <div key={slot} className="flex items-center justify-between">
              <span className="text-gray-600 text-[10px] font-mono">Slot {slot + 1}</span>
              <div className="flex gap-1">
                <button
                  onClick={() => handleSave(slot)}
                  disabled={!romName}
                  title={`Save slot ${slot + 1}`}
                  className="px-2 py-0.5 rounded border border-green-900 bg-green-950/50
                    hover:bg-green-900/60 text-green-400 text-[10px] font-mono
                    disabled:opacity-25 disabled:cursor-not-allowed transition-colors"
                >SAV</button>
                <button
                  onClick={() => handleLoad(slot, saveSlots)}
                  disabled={saveSlots[slot] === null}
                  title={`Load slot ${slot + 1}`}
                  className="px-2 py-0.5 rounded border border-blue-900 bg-blue-950/50
                    hover:bg-blue-900/60 text-blue-400 text-[10px] font-mono
                    disabled:opacity-25 disabled:cursor-not-allowed transition-colors"
                >
                  {saveSlots[slot] ? "LOAD" : "----"}
                </button>
              </div>
            </div>
          ))}
        </div>

        {/* Status */}
        <div className="flex items-center gap-1.5">
          <span className={`w-1.5 h-1.5 rounded-full ${running ? "bg-green-500 animate-pulse" : "bg-gray-700"}`} />
          <span className="text-gray-600 text-[10px]">{running ? "Running" : "Idle"}</span>
        </div>
      </aside>

    </div>
  );
}
