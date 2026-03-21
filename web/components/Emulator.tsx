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
import { systemAccent } from "@/lib/theme";


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
  const [saveSlots,      setSaveSlots]      = useState<(Uint8Array | null)[]>([null, null, null]);
  const [saveTimestamps, setSaveTimestamps] = useState<(number | null)[]>([null, null, null]);

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
          if (raw) next[i] = Uint8Array.from(atob(raw), (c) => c.charCodeAt(0));
        } catch { /* ignore corrupt entries */ }
      }
      return next;
    });
    setSaveTimestamps(() => {
      const next: (number | null)[] = [null, null, null];
      for (let i = 0; i < 3; i++) {
        try {
          const raw = localStorage.getItem(lsKey(i) + "_ts");
          if (raw) next[i] = Number(raw);
        } catch { /* ignore */ }
      }
      return next;
    });
  }, [romName, lsKey]);

  const persistSlot = useCallback(
    (slot: number, data: Uint8Array, ts: number) => {
      try {
        const b64 = btoa(Array.from(data, (b) => String.fromCharCode(b)).join(""));
        localStorage.setItem(lsKey(slot), b64);
        localStorage.setItem(lsKey(slot) + "_ts", String(ts));
      } catch {
        // quota exceeded or unavailable — silently skip
      }
    },
    [lsKey]
  );

  const handleSave = useCallback(
    (slot: number) => {
      const emu = emulatorRef.current;
      if (!emu) return;
      const data = emu.save_state();
      const ts = Date.now();
      setSaveSlots((prev) => { const next = [...prev] as (Uint8Array | null)[];  next[slot] = data; return next; });
      setSaveTimestamps((prev) => { const next = [...prev] as (number | null)[]; next[slot] = ts;   return next; });
      persistSlot(slot, data, ts);
    },
    [persistSlot]
  );

  // Push save: shift slot[0]→[1]→[2] (dropping oldest), save to slot[0]
  const handlePushSave = useCallback(() => {
    const emu = emulatorRef.current;
    if (!emu) return;
    const data = emu.save_state();
    const ts = Date.now();
    setSaveSlots((prev) => [data, prev[0], prev[1]] as (Uint8Array | null)[]);
    setSaveTimestamps((prev) => {
      persistSlot(0, data, ts);
      if (saveSlots[0]) persistSlot(1, saveSlots[0], prev[0] ?? ts);
      if (saveSlots[1]) persistSlot(2, saveSlots[1], prev[1] ?? ts);
      return [ts, prev[0], prev[1]];
    });
  }, [persistSlot, saveSlots]);

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

  // Spacebar → push save
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code === "Space" && !e.repeat) { e.preventDefault(); handlePushSave(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [handlePushSave]);

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


  const panelHeader = (label: string) => (
    <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8 }}>
      <div style={{ flex: 1, height: 1, background: "linear-gradient(90deg, transparent, #2a2a2a)" }} />
      <span style={{ color: "#333", fontSize: 9, letterSpacing: "0.25em", fontFamily: "monospace", textTransform: "uppercase" }}>{label}</span>
      <div style={{ flex: 1, height: 1, background: "linear-gradient(90deg, #2a2a2a, transparent)" }} />
    </div>
  );

  const sysAccent = systemAccent(system);

  return (
    <div className="flex items-start gap-5">

      {/* ── Left panel ── */}
      <aside className="flex flex-col gap-3 w-40 pt-1">

        {/* Load ROM button */}
        <div className="flex flex-col gap-1.5">
          <label
            htmlFor="rom-input"
            style={{
              display: "block", textAlign: "center",
              padding: "8px 12px", borderRadius: 8, cursor: "pointer",
              fontSize: 11, fontWeight: 700, letterSpacing: "0.08em",
              color: "#4ade80",
              background: "linear-gradient(180deg, rgba(20,60,20,0.6) 0%, rgba(10,30,10,0.4) 100%)",
              border: "1px solid #1a4a1a",
              boxShadow: "0 0 12px rgba(74,222,128,0.08), inset 0 1px 0 rgba(74,222,128,0.1)",
              transition: "all 0.2s",
            }}
          >
            {romName ? "⏏ Change ROM" : "▶ Load ROM"}
          </label>
          <input id="rom-input" type="file" accept=".gb,.gbc,.nes"
            className="hidden" onChange={handleRomLoad} />
          {loadError && <p className="text-red-400 text-[10px] leading-tight">{loadError}</p>}
        </div>

        {/* Keyboard reference */}
        <div style={{
          borderRadius: 8, padding: "10px 12px",
          background: "rgba(10,10,10,0.6)",
          border: "1px solid #1a1a1a",
          boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03)",
        }}>
          {panelHeader("Keys")}
          {/* WASD cluster */}
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 2, marginBottom: 8 }}>
            {[
              [{ k: "W", sub: "▲" }],
              [{ k: "A", sub: "◀" }, { k: "S", sub: "▼" }, { k: "D", sub: "▶" }],
            ].map((row, ri) => (
              <div key={ri} style={{ display: "flex", gap: 2 }}>
                {row.map(({ k, sub }) => (
                  <div key={k} style={{
                    width: 26, height: 26, borderRadius: 5,
                    background: "linear-gradient(180deg, #1e1e1e 0%, #161616 100%)",
                    border: "1px solid #2a2a2a", borderBottom: "2px solid #0a0a0a",
                    display: "flex", flexDirection: "column",
                    alignItems: "center", justifyContent: "center",
                    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.05)",
                  }}>
                    <span style={{ color: "#3a3a3a", fontSize: 9, fontFamily: "monospace", fontWeight: 700, lineHeight: 1 }}>{k}</span>
                    <span style={{ color: "#252525", fontSize: 6, lineHeight: 1 }}>{sub}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>

          {/* IJKL cluster */}
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 2 }}>
            {[
              [{ k: "I", label: "SEL" }],
              [{ k: "J", label: "A" }, { k: "K", label: "B" }, { k: "L", label: "STA" }],
            ].map((row, ri) => (
              <div key={ri} style={{ display: "flex", gap: 2 }}>
                {row.map(({ k, label }) => (
                  <div key={k} style={{
                    width: 26, height: 26, borderRadius: 5,
                    background: "linear-gradient(180deg, #1e1e1e 0%, #161616 100%)",
                    border: "1px solid #2a2a2a", borderBottom: "2px solid #0a0a0a",
                    display: "flex", flexDirection: "column",
                    alignItems: "center", justifyContent: "center",
                    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.05)",
                  }}>
                    <span style={{ color: "#3a3a3a", fontSize: 9, fontFamily: "monospace", fontWeight: 700, lineHeight: 1 }}>{k}</span>
                    <span style={{ color: "#252525", fontSize: 6, lineHeight: 1 }}>{label}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </div>

        {/* Status indicator */}
        <div style={{
          display: "flex", alignItems: "center", gap: 8, padding: "8px 12px",
          borderRadius: 8, background: "rgba(10,10,10,0.4)", border: "1px solid #181818",
        }}>
          <div style={{
            width: 6, height: 6, borderRadius: "50%",
            background: running ? sysAccent : "#1a1a1a",
            boxShadow: running ? `0 0 6px ${sysAccent}` : "none",
            animation: running ? "led-pulse 2s ease-in-out infinite" : "none",
            flexShrink: 0,
          }} />
          <span style={{ color: running ? "#2a3a2a" : "#1f1f1f", fontSize: 9,
            fontFamily: "monospace", letterSpacing: "0.15em", textTransform: "uppercase" }}>
            {running ? "Running" : "Standby"}
          </span>
        </div>
      </aside>

      {/* ── Center: screen + controller ── */}
      <div className="flex flex-col" style={{
        filter: "drop-shadow(0 20px 60px rgba(0,0,0,0.8))",
      }}>
        <Screen frameBuffer={frameBuffer} width={screenW} height={screenH} system={system} />
        <Controls onButton={handleButton} />
      </div>

      {/* ── Right panel ── */}
      <aside className="flex flex-col gap-3 w-40 pt-1">

        {/* System info */}
        <div style={{
          borderRadius: 8, padding: "10px 12px",
          background: "rgba(10,10,10,0.6)",
          border: `1px solid ${system ? sysAccent + "33" : "#1a1a1a"}`,
          boxShadow: system ? `0 0 20px ${sysAccent}0d, inset 0 1px 0 rgba(255,255,255,0.03)` : "inset 0 1px 0 rgba(255,255,255,0.03)",
          transition: "border-color 0.4s, box-shadow 0.4s",
        }}>
          {panelHeader("System")}
          {system ? (
            <>
              <div style={{
                display: "inline-block", padding: "2px 8px", borderRadius: 4, marginBottom: 8,
                background: system === "NES" ? "rgba(255,68,68,0.15)" : "rgba(68,255,136,0.12)",
                border: `1px solid ${sysAccent}44`,
                color: sysAccent, fontSize: 11, fontWeight: 800,
                fontFamily: "monospace", letterSpacing: "0.1em",
              }}>{system}</div>
              <p style={{ color: "#252525", fontSize: 9, fontFamily: "monospace",
                lineHeight: 1.7, wordBreak: "break-all" }}>{romName}</p>
              <div style={{ marginTop: 8, paddingTop: 8, borderTop: "1px solid #181818" }}>
                {(system === "NES"
                  ? [["256×240", ""], ["60 fps", ""], ["Ricoh 2A03", ""]]
                  : [["160×144", ""], ["59.7 fps", ""], ["Sharp LR35902", ""]]
                ).map(([val]) => (
                  <div key={val} style={{ color: "#222", fontSize: 9, fontFamily: "monospace", marginBottom: 2 }}>
                    {val}
                  </div>
                ))}
              </div>
            </>
          ) : (
            <span style={{ color: "#1f1f1f", fontSize: 9, fontFamily: "monospace" }}>No ROM loaded</span>
          )}
        </div>

        {/* Save slots */}
        <div style={{
          borderRadius: 8, padding: "10px 12px",
          background: "rgba(10,10,10,0.6)",
          border: "1px solid #1a1a1a",
          boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03)",
        }}>
          {panelHeader("Save States")}
          <button
            onClick={handlePushSave}
            disabled={!romName}
            style={{
              width: "100%", marginBottom: 8,
              padding: "4px 0", borderRadius: 4, fontSize: 9, fontFamily: "monospace",
              letterSpacing: "0.1em",
              color: romName ? "#facc15" : "#1a1a1a",
              background: "rgba(40,35,5,0.5)", border: "1px solid #3a3000",
              cursor: romName ? "pointer" : "not-allowed",
              transition: "all 0.15s",
            }}
            title="Push save: save to ①, shift ①→② ②→③, drop ③"
          >PUSH SAVE</button>
          {([0, 1, 2] as const).map((slot) => (
            <div key={slot} style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: slot < 2 ? 7 : 0 }}>
              <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
                <span style={{ color: "#252525", fontSize: 9, fontFamily: "monospace" }}>
                  {["①","②","③"][slot]}
                </span>
                {saveTimestamps[slot] && (
                  <span style={{ color: "#1e1e1e", fontSize: 7, fontFamily: "monospace" }}>
                    {new Date(saveTimestamps[slot]!).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                  </span>
                )}
              </div>
              <div style={{ display: "flex", gap: 4 }}>
                <button
                  onClick={() => handleSave(slot)}
                  disabled={!romName}
                  style={{
                    padding: "2px 7px", borderRadius: 4, fontSize: 9, fontFamily: "monospace",
                    color: romName ? "#22c55e" : "#1a1a1a",
                    background: "rgba(20,50,20,0.4)", border: "1px solid #1a3a1a",
                    cursor: romName ? "pointer" : "not-allowed",
                    transition: "all 0.15s",
                  }}
                  title={`Save slot ${slot + 1}`}
                >SAV</button>
                <button
                  onClick={() => handleLoad(slot, saveSlots)}
                  disabled={saveSlots[slot] === null}
                  style={{
                    padding: "2px 7px", borderRadius: 4, fontSize: 9, fontFamily: "monospace",
                    color: saveSlots[slot] ? "#3b82f6" : "#1a1a1a",
                    background: "rgba(10,20,50,0.4)", border: "1px solid #1a1a3a",
                    cursor: saveSlots[slot] ? "pointer" : "not-allowed",
                    transition: "all 0.15s",
                  }}
                  title={`Load slot ${slot + 1}`}
                >{saveSlots[slot] ? "LOAD" : "----"}</button>
              </div>
            </div>
          ))}
        </div>
      </aside>

    </div>
  );
}
