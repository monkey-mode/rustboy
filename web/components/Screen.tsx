"use client";

import { useEffect, useRef } from "react";

interface ScreenProps {
  frameBuffer: Uint8Array | null;
  width: number;
  height: number;
}

const SCALE = 3;

export default function Screen({ frameBuffer, width, height }: ScreenProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !frameBuffer) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const imageData = ctx.createImageData(width, height);
    imageData.data.set(frameBuffer);
    ctx.putImageData(imageData, 0, 0);
  }, [frameBuffer, width, height]);

  return (
    <canvas
      ref={canvasRef}
      width={width}
      height={height}
      style={{
        width: width * SCALE,
        height: height * SCALE,
        imageRendering: "pixelated",
        display: "block",
      }}
      className="border-4 border-green-700 rounded"
      aria-label="Emulator screen"
    />
  );
}
