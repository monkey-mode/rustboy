"use client";

import { useEffect, useCallback } from "react";

export const BUTTON = {
  RIGHT:  0,
  LEFT:   1,
  UP:     2,
  DOWN:   3,
  A:      4,
  B:      5,
  SELECT: 6,
  START:  7,
} as const;

export type ButtonIndex = (typeof BUTTON)[keyof typeof BUTTON];

interface ControlsProps {
  onButton: (button: ButtonIndex, pressed: boolean) => void;
}

export default function Controls({ onButton }: ControlsProps) {
  // Keyboard handler
  const handleKey = useCallback(
    (e: KeyboardEvent, pressed: boolean) => {
      let button: ButtonIndex | null = null;
      switch (e.key) {
        case "ArrowRight": case "d": case "D": button = BUTTON.RIGHT;  break;
        case "ArrowLeft":  case "a": case "A": button = BUTTON.LEFT;   break;
        case "ArrowUp":    case "w": case "W": button = BUTTON.UP;     break;
        case "ArrowDown":  case "s": case "S": button = BUTTON.DOWN;   break;
        case "z": case "Z": case "j": case "J": button = BUTTON.A;      break;
        case "x": case "X": case "k": case "K": button = BUTTON.B;      break;
        case "i": case "I": case "Shift":        button = BUTTON.SELECT; break;
        case "l": case "L": case "Enter":        button = BUTTON.START;  break;
        default: return;
      }
      e.preventDefault();
      onButton(button, pressed);
    },
    [onButton]
  );

  useEffect(() => {
    const down = (e: KeyboardEvent) => handleKey(e, true);
    const up   = (e: KeyboardEvent) => handleKey(e, false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup",   up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup",   up);
    };
  }, [handleKey]);

  const press   = (btn: ButtonIndex) => () => onButton(btn, true);
  const release = (btn: ButtonIndex) => () => onButton(btn, false);

  const dBtn = (label: string, btn: ButtonIndex, extra = "") =>
    <button
      className={`select-none active:scale-90 transition-all duration-75 w-9 h-9
        bg-gray-800 hover:bg-gray-700 active:bg-gray-600
        border border-gray-600 rounded-lg flex items-center justify-center
        text-gray-400 text-[11px] font-medium ${extra}`}
      onPointerDown={press(btn)} onPointerUp={release(btn)} onPointerLeave={release(btn)}
      aria-label={label}
    >{label}</button>;

  return (
    // Controller bar — same 480px width as the screen
    <div
      className="flex items-center justify-between px-4 py-3 rounded-b-xl
        bg-gray-900/90 border-x border-b border-gray-800/60"
      style={{ width: 480 }}
    >
      {/* D-pad */}
      <div className="grid grid-cols-3 gap-0.5" style={{ width: 112 }}>
        <div />{dBtn("▲", BUTTON.UP)}<div />
        {dBtn("◀", BUTTON.LEFT)}
        <div className="w-9 h-9 bg-gray-900 rounded-lg border border-gray-800" />
        {dBtn("▶", BUTTON.RIGHT)}
        <div />{dBtn("▼", BUTTON.DOWN)}<div />
      </div>

      {/* Select / Start */}
      <div className="flex gap-3">
        <button
          className="select-none active:scale-95 transition-all text-gray-500 hover:text-gray-300
            text-[10px] tracking-widest uppercase px-3 py-1.5 rounded-full
            border border-gray-700 hover:border-gray-500 bg-gray-900"
          onPointerDown={press(BUTTON.SELECT)} onPointerUp={release(BUTTON.SELECT)} onPointerLeave={release(BUTTON.SELECT)}
          aria-label="Select"
        >SEL</button>
        <button
          className="select-none active:scale-95 transition-all text-gray-500 hover:text-gray-300
            text-[10px] tracking-widest uppercase px-3 py-1.5 rounded-full
            border border-gray-700 hover:border-gray-500 bg-gray-900"
          onPointerDown={press(BUTTON.START)} onPointerUp={release(BUTTON.START)} onPointerLeave={release(BUTTON.START)}
          aria-label="Start"
        >STA</button>
      </div>

      {/* A / B */}
      <div className="flex items-end gap-3">
        <button
          className="select-none active:scale-90 transition-all duration-75 w-10 h-10
            rounded-full bg-orange-700 hover:bg-orange-600 font-bold text-white text-sm shadow-lg"
          style={{ marginBottom: 8 }}
          onPointerDown={press(BUTTON.B)} onPointerUp={release(BUTTON.B)} onPointerLeave={release(BUTTON.B)}
          aria-label="B"
        >B</button>
        <button
          className="select-none active:scale-90 transition-all duration-75 w-12 h-12
            rounded-full bg-red-600 hover:bg-red-500 font-bold text-white text-sm shadow-lg"
          onPointerDown={press(BUTTON.A)} onPointerUp={release(BUTTON.A)} onPointerLeave={release(BUTTON.A)}
          aria-label="A"
        >A</button>
      </div>
    </div>
  );
}
