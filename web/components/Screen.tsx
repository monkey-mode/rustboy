"use client";

import { useEffect, useRef } from "react";

interface ScreenProps {
  frameBuffer: Uint8Array | null;
  width: number;
  height: number;
}

// GB  160×144 → 3.0× = 480×432 (exact integer scale)
// NES 256×240 → 1.875× = 480×432  (same physical size, letterbox-free)
const DISPLAY_W = 480;
const DISPLAY_H = 432;

export default function Screen({ frameBuffer, width, height }: ScreenProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !frameBuffer) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Draw the game frame at native resolution into an offscreen buffer,
    // then scale it up to fill the display canvas.
    const imageData = ctx.createImageData(width, height);
    imageData.data.set(frameBuffer);

    const offscreen = new OffscreenCanvas(width, height);
    const offCtx = offscreen.getContext("2d")!;
    offCtx.putImageData(imageData, 0, 0);

    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(offscreen, 0, 0, DISPLAY_W, DISPLAY_H);
  }, [frameBuffer, width, height]);

  return (
    <div
      className="relative rounded-xl overflow-hidden ring-1 ring-green-900/60"
      style={{
        width: DISPLAY_W,
        height: DISPLAY_H,
        boxShadow: "0 0 60px rgba(74,222,128,0.08), 0 0 0 1px rgba(74,222,128,0.1)",
        background: "#000",
      }}
    >
      {/* Scanline overlay */}
      <div
        className="absolute inset-0 pointer-events-none z-10"
        style={{
          backgroundImage:
            "repeating-linear-gradient(0deg, transparent, transparent 1px, rgba(0,0,0,0.18) 1px, rgba(0,0,0,0.18) 2px)",
          mixBlendMode: "multiply",
        }}
      />
      <canvas
        ref={canvasRef}
        width={DISPLAY_W}
        height={DISPLAY_H}
        style={{ imageRendering: "pixelated", display: "block" }}
        aria-label="Emulator screen"
      />
      {!frameBuffer && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 text-gray-700">
          <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="2" y="6" width="20" height="14" rx="2" />
            <path d="M8 6V4M16 6V4" />
            <circle cx="8" cy="13" r="1" fill="currentColor" />
            <circle cx="16" cy="13" r="1" fill="currentColor" />
          </svg>
          <span className="text-xs tracking-widest uppercase">Load a ROM to play</span>
        </div>
      )}
    </div>
  );
}
