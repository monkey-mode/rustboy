"use client";

import { useEffect, useCallback } from "react";

// Button indices (match Rust set_joypad signature)
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

/**
 * On-screen D-pad + action buttons and keyboard event listeners.
 *
 * Keyboard mapping:
 *   ArrowRight → Right    ArrowLeft → Left
 *   ArrowUp    → Up       ArrowDown → Down
 *   Z          → A        X         → B
 *   Enter      → Start    Shift     → Select
 */
export default function Controls({ onButton }: ControlsProps) {
  const handleKey = useCallback(
    (e: KeyboardEvent, pressed: boolean) => {
      let button: ButtonIndex | null = null;
      switch (e.key) {
        case "ArrowRight": button = BUTTON.RIGHT;  break;
        case "ArrowLeft":  button = BUTTON.LEFT;   break;
        case "ArrowUp":    button = BUTTON.UP;     break;
        case "ArrowDown":  button = BUTTON.DOWN;   break;
        case "z": case "Z": button = BUTTON.A;     break;
        case "x": case "X": button = BUTTON.B;     break;
        case "Enter":      button = BUTTON.START;  break;
        case "Shift":      button = BUTTON.SELECT; break;
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

  // Touch / click helpers for on-screen buttons
  const press   = (btn: ButtonIndex) => () => onButton(btn, true);
  const release = (btn: ButtonIndex) => () => onButton(btn, false);

  const btnClass =
    "select-none active:scale-95 transition-transform cursor-pointer " +
    "bg-gray-700 hover:bg-gray-600 rounded-full font-bold text-xs text-gray-200 " +
    "flex items-center justify-center";

  return (
    <div className="flex gap-8 items-center mt-4">
      {/* D-pad */}
      <div className="grid grid-cols-3 grid-rows-3 gap-1 w-28 h-28">
        {/* row 1 */}
        <div />
        <button
          className={`${btnClass} w-9 h-9`}
          onPointerDown={press(BUTTON.UP)}
          onPointerUp={release(BUTTON.UP)}
          onPointerLeave={release(BUTTON.UP)}
          aria-label="Up"
        >
          ▲
        </button>
        <div />
        {/* row 2 */}
        <button
          className={`${btnClass} w-9 h-9`}
          onPointerDown={press(BUTTON.LEFT)}
          onPointerUp={release(BUTTON.LEFT)}
          onPointerLeave={release(BUTTON.LEFT)}
          aria-label="Left"
        >
          ◀
        </button>
        <div className="w-9 h-9 bg-gray-800 rounded" />
        <button
          className={`${btnClass} w-9 h-9`}
          onPointerDown={press(BUTTON.RIGHT)}
          onPointerUp={release(BUTTON.RIGHT)}
          onPointerLeave={release(BUTTON.RIGHT)}
          aria-label="Right"
        >
          ▶
        </button>
        {/* row 3 */}
        <div />
        <button
          className={`${btnClass} w-9 h-9`}
          onPointerDown={press(BUTTON.DOWN)}
          onPointerUp={release(BUTTON.DOWN)}
          onPointerLeave={release(BUTTON.DOWN)}
          aria-label="Down"
        >
          ▼
        </button>
        <div />
      </div>

      {/* Select / Start */}
      <div className="flex gap-3 flex-col items-center">
        <button
          className={`${btnClass} w-14 h-6 rounded-full`}
          onPointerDown={press(BUTTON.SELECT)}
          onPointerUp={release(BUTTON.SELECT)}
          onPointerLeave={release(BUTTON.SELECT)}
          aria-label="Select"
        >
          SELECT
        </button>
        <button
          className={`${btnClass} w-14 h-6 rounded-full`}
          onPointerDown={press(BUTTON.START)}
          onPointerUp={release(BUTTON.START)}
          onPointerLeave={release(BUTTON.START)}
          aria-label="Start"
        >
          START
        </button>
      </div>

      {/* A / B */}
      <div className="flex gap-4 items-center">
        <button
          className={`${btnClass} w-12 h-12 bg-red-800 hover:bg-red-700`}
          onPointerDown={press(BUTTON.B)}
          onPointerUp={release(BUTTON.B)}
          onPointerLeave={release(BUTTON.B)}
          aria-label="B"
        >
          B
        </button>
        <button
          className={`${btnClass} w-12 h-12 bg-red-600 hover:bg-red-500`}
          onPointerDown={press(BUTTON.A)}
          onPointerUp={release(BUTTON.A)}
          onPointerLeave={release(BUTTON.A)}
          aria-label="A"
        >
          A
        </button>
      </div>
    </div>
  );
}
