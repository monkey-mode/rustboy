"use client";

import { useEffect, useRef } from "react";

interface ScreenProps {
  /** Raw RGBA bytes for a 160×144 frame. Must be exactly 92160 bytes. */
  frameBuffer: Uint8Array | null;
}

const SCREEN_W = 160;
const SCREEN_H = 144;
const SCALE   = 3;

/**
 * Renders Game Boy frames onto a 160×144 canvas scaled up 3× via CSS.
 * `image-rendering: pixelated` keeps the sharp pixel look.
 */
export default function Screen({ frameBuffer }: ScreenProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !frameBuffer) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const imageData = ctx.createImageData(SCREEN_W, SCREEN_H);
    imageData.data.set(frameBuffer);
    ctx.putImageData(imageData, 0, 0);
  }, [frameBuffer]);

  return (
    <canvas
      ref={canvasRef}
      width={SCREEN_W}
      height={SCREEN_H}
      style={{
        width: SCREEN_W * SCALE,
        height: SCREEN_H * SCALE,
        imageRendering: "pixelated",
        display: "block",
      }}
      className="border-4 border-green-700 rounded"
      aria-label="Game Boy screen"
    />
  );
}
