"use client";

import { useEffect, useRef } from "react";
import { systemAccent, systemGlow } from "@/lib/theme";

interface ScreenProps {
  frameBuffer: Uint8Array | null;
  width: number;
  height: number;
  system: "GB" | "NES" | null;
}

const DISPLAY_W = 480;
const DISPLAY_H = 432;

const SCREW_POSITIONS = [
  { top: 10, left: 10 },
  { top: 10, right: 10 },
  { bottom: 10, left: 10 },
  { bottom: 10, right: 10 },
] as const;

export default function Screen({ frameBuffer, width, height, system }: ScreenProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const isOn = frameBuffer !== null;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !frameBuffer) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const imageData = ctx.createImageData(width, height);
    imageData.data.set(frameBuffer);

    const offscreen = new OffscreenCanvas(width, height);
    const offCtx = offscreen.getContext("2d")!;
    offCtx.putImageData(imageData, 0, 0);

    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(offscreen, 0, 0, DISPLAY_W, DISPLAY_H);
  }, [frameBuffer, width, height]);

  const accentColor = systemAccent(system);
  const accentGlow  = systemGlow(system);

  return (
    /* ── Outer TV casing ── */
    <div style={{
      background: "linear-gradient(160deg, #2c2c2c 0%, #1a1a1a 45%, #222 100%)",
      borderRadius: "18px 18px 0 0",
      padding: "20px 20px 14px",
      position: "relative",
      boxShadow: [
        "inset 0 1px 0 rgba(255,255,255,0.07)",
        "inset 0 -1px 0 rgba(0,0,0,0.6)",
        "0 0 0 1px #0d0d0d",
        "0 -4px 0 #111",
        "0 -30px 60px rgba(0,0,0,0.5)",
      ].join(", "),
    }}>

      {/* Corner screws */}
      {SCREW_POSITIONS.map((pos, i) => (
        <div key={i} style={{
          position: "absolute", ...pos,
          width: 8, height: 8, borderRadius: "50%",
          background: "radial-gradient(circle at 35% 30%, #505050, #1c1c1c)",
          boxShadow: "inset 0 0.5px 1px rgba(255,255,255,0.1), 0 1px 3px rgba(0,0,0,0.7)",
        }}>
          {/* Phillips head */}
          <div style={{ position:"absolute", inset:0, display:"flex", alignItems:"center", justifyContent:"center" }}>
            <div style={{ position:"absolute", width:4, height:0.5, background:"rgba(0,0,0,0.5)" }} />
            <div style={{ position:"absolute", width:0.5, height:4, background:"rgba(0,0,0,0.5)" }} />
          </div>
        </div>
      ))}

      {/* Inner screen bezel */}
      <div style={{
        background: "#080808",
        borderRadius: 6,
        padding: 7,
        boxShadow: [
          "inset 0 0 0 1px #000",
          "inset 2px 2px 12px rgba(0,0,0,0.9)",
          "inset -1px -1px 6px rgba(255,255,255,0.02)",
        ].join(", "),
      }}>
        {/* Screen area */}
        <div style={{ position: "relative", borderRadius: 3, overflow: "hidden" }}>
          {/* Canvas */}
          <canvas
            ref={canvasRef}
            width={DISPLAY_W}
            height={DISPLAY_H}
            style={{ imageRendering: "pixelated", display: "block" }}
            aria-label="Emulator screen"
          />

          {/* Scanline overlay */}
          <div style={{
            position: "absolute", inset: 0, pointerEvents: "none", zIndex: 10,
            backgroundImage: "repeating-linear-gradient(0deg, transparent, transparent 1px, rgba(0,0,0,0.22) 1px, rgba(0,0,0,0.22) 2px)",
          }} />

          {/* Screen glow tint when running */}
          {isOn && (
            <div style={{
              position: "absolute", inset: 0, pointerEvents: "none", zIndex: 9,
              boxShadow: `inset 0 0 60px ${accentGlow}`,
            }} />
          )}

          {/* CRT vignette */}
          <div style={{
            position: "absolute", inset: 0, pointerEvents: "none", zIndex: 11,
            background: "radial-gradient(ellipse at 50% 50%, transparent 60%, rgba(0,0,0,0.55) 100%)",
          }} />

          {/* Placeholder when no ROM */}
          {!frameBuffer && (
            <div style={{
              position: "absolute", inset: 0, zIndex: 12,
              display: "flex", flexDirection: "column",
              alignItems: "center", justifyContent: "center", gap: 14,
              background: "#000",
            }}>
              {/* Pixel art game controller icon */}
              <div style={{ display: "flex", flexDirection: "column", gap: 2, opacity: 0.25 }}>
                {[
                  "  ████████  ",
                  " ██      ██ ",
                  "██ ██  ██ ██",
                  "██ ██  ██ ██",
                  " ██      ██ ",
                  "  ████████  ",
                ].map((row, i) => (
                  <div key={i} style={{ display: "flex", gap: 0 }}>
                    {row.split("").map((ch, j) => (
                      <div key={j} style={{
                        width: 6, height: 6,
                        background: ch === "█" ? "#4ade80" : "transparent",
                      }} />
                    ))}
                  </div>
                ))}
              </div>
              <span style={{
                color: "#1a2a1a", fontSize: 11,
                letterSpacing: "0.25em", textTransform: "uppercase",
                fontFamily: "monospace",
              }}>INSERT CARTRIDGE</span>
            </div>
          )}
        </div>
      </div>

      {/* ── Bottom brand strip ── */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        marginTop: 12, padding: "0 6px",
      }}>
        {/* Left speaker grille */}
        <div style={{ display: "flex", gap: 2.5 }}>
          {Array.from({ length: 7 }).map((_, i) => (
            <div key={i} style={{ width: 2, height: 12, background: "#1a1a1a", borderRadius: 1,
              boxShadow: "inset 0 1px 0 rgba(0,0,0,0.8)" }} />
          ))}
        </div>

        {/* Brand name */}
        <span style={{
          color: "#2a2a2a", fontSize: 9, letterSpacing: "0.4em",
          fontFamily: "monospace", textTransform: "uppercase", userSelect: "none",
        }}>RUSTBOY</span>

        {/* Right speaker + LED */}
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <div style={{ display: "flex", gap: 2.5 }}>
            {Array.from({ length: 7 }).map((_, i) => (
              <div key={i} style={{ width: 2, height: 12, background: "#1a1a1a", borderRadius: 1,
                boxShadow: "inset 0 1px 0 rgba(0,0,0,0.8)" }} />
            ))}
          </div>
          {/* Power LED */}
          <div style={{
            width: 6, height: 6, borderRadius: "50%",
            background: isOn ? accentColor : "#111",
            boxShadow: isOn ? `0 0 6px ${accentColor}, 0 0 12px ${accentGlow}` : "inset 0 1px 0 rgba(0,0,0,0.5)",
            animation: isOn ? "led-pulse 2s ease-in-out infinite" : "none",
            transition: "background 0.4s, box-shadow 0.4s",
          }} />
        </div>
      </div>
    </div>
  );
}
