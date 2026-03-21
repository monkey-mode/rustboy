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

function capture(e: React.PointerEvent) {
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
}

const INDENT_STYLE: React.CSSProperties = {
  width: 40, height: 3, borderRadius: 2,
  background: "linear-gradient(90deg, #141414, #1e1e1e, #141414)",
  boxShadow: "inset 0 1px 0 rgba(0,0,0,0.8)",
};

export default function Controls({ onButton }: ControlsProps) {
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

  const onDown = (btn: ButtonIndex, scale = "0.9") =>
    (e: React.PointerEvent<HTMLButtonElement>) => {
      capture(e); press(btn)();
      e.currentTarget.style.transform = `scale(${scale})`;
      e.currentTarget.style.filter = "brightness(1.3)";
    };
  const onUp = (btn: ButtonIndex) =>
    (e: React.PointerEvent<HTMLButtonElement>) => {
      release(btn)();
      e.currentTarget.style.transform = "scale(1)";
      e.currentTarget.style.filter = "brightness(1)";
    };

  // D-pad button — no individual border; the grid container provides the seam color
  const dBtn = (label: string, btn: ButtonIndex) => (
    <button
      style={{
        width: 36, height: 36, borderRadius: 0,
        background: "linear-gradient(180deg, #2e2e2e 0%, #252525 100%)",
        border: "none",
        display: "flex", alignItems: "center", justifyContent: "center",
        color: "#4a4a4a", fontSize: 11, cursor: "pointer",
        userSelect: "none", touchAction: "none",
        transition: "background 60ms",
      }}
      onPointerDown={(e) => { capture(e); press(btn)(); (e.currentTarget as HTMLButtonElement).style.background = "#3a3a3a"; }}
      onPointerUp={(e) => { release(btn)(); (e.currentTarget as HTMLButtonElement).style.background = ""; }}
      onPointerCancel={(e) => { release(btn)(); (e.currentTarget as HTMLButtonElement).style.background = ""; }}
      aria-label={label}
    >{label}</button>
  );

  // Face button (round, colored)
  const faceBtn = (
    label: string,
    btn: ButtonIndex,
    bg: string,
    size: number,
    shadow: string,
    mb = 0
  ) => (
    <button
      style={{
        width: size, height: size, borderRadius: "50%",
        background: bg,
        border: "none",
        boxShadow: shadow,
        color: "rgba(255,255,255,0.9)", fontWeight: 800, fontSize: size * 0.28,
        cursor: "pointer", userSelect: "none", touchAction: "none",
        marginBottom: mb,
        transition: "transform 60ms, filter 60ms",
      }}
      onPointerDown={onDown(btn)}
      onPointerUp={onUp(btn)}
      onPointerCancel={onUp(btn)}
      aria-label={label}
    >{label}</button>
  );

  return (
    <div style={{
      width: "100%",
      boxSizing: "border-box",
      background: "linear-gradient(180deg, #222 0%, #1c1c1c 50%, #202020 100%)",
      borderRadius: "0 0 28px 28px",
      padding: "18px 28px 22px",
      position: "relative",
      boxShadow: [
        "0 0 0 1px #0d0d0d",
        "0 12px 40px rgba(0,0,0,0.7)",
        "inset 0 1px 0 rgba(255,255,255,0.04)",
        "inset 0 -2px 4px rgba(0,0,0,0.5)",
      ].join(", "),
    }}>
      {/* Subtle grip texture lines */}
      <div style={{
        position: "absolute", top: 0, left: "50%", transform: "translateX(-50%)",
        width: 80, height: 3,
        background: "linear-gradient(90deg, transparent, #2a2a2a 20%, #2a2a2a 80%, transparent)",
        borderRadius: "0 0 4px 4px",
      }} />

      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>

        {/* ── D-pad ── */}
        <div style={{
          display: "grid", gridTemplateColumns: "36px 36px 36px",
          gap: 1, background: "#111", borderRadius: 8, overflow: "hidden",
          boxShadow: "0 3px 8px rgba(0,0,0,0.7), inset 0 1px 0 rgba(255,255,255,0.04)",
        }}>
          <div style={{ background: "#1a1a1a" }} />
          {dBtn("▲", BUTTON.UP)}
          <div style={{ background: "#1a1a1a" }} />
          {dBtn("◀", BUTTON.LEFT)}
          <div style={{ background: "linear-gradient(135deg, #222, #1e1e1e)" }} />
          {dBtn("▶", BUTTON.RIGHT)}
          <div style={{ background: "#1a1a1a" }} />
          {dBtn("▼", BUTTON.DOWN)}
          <div style={{ background: "#1a1a1a" }} />
        </div>

        {/* ── Select / Start ── */}
        <div style={{ display: "flex", flexDirection: "column", gap: 12, alignItems: "center" }}>
          <div style={INDENT_STYLE} />
          <div style={{ display: "flex", gap: 12 }}>
            {(["SEL", "STA"] as const).map((label, i) => {
              const btn = i === 0 ? BUTTON.SELECT : BUTTON.START;
              return (
                <button
                  key={label}
                  style={{
                    padding: "5px 12px",
                    background: "linear-gradient(180deg, #2a2a2a 0%, #1e1e1e 100%)",
                    border: "1px solid #111",
                    borderRadius: 20,
                    boxShadow: "0 2px 4px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.05)",
                    color: "#4a4a4a", fontSize: 9,
                    letterSpacing: "0.15em", fontFamily: "monospace",
                    cursor: "pointer", userSelect: "none", touchAction: "none",
                    transition: "transform 60ms",
                  }}
                  onPointerDown={onDown(btn, "0.92")}
                  onPointerUp={onUp(btn)}
                  onPointerCancel={onUp(btn)}
                  aria-label={label === "SEL" ? "Select" : "Start"}
                >{label}</button>
              );
            })}
          </div>
          <div style={INDENT_STYLE} />
        </div>

        {/* ── A / B buttons ── */}
        <div style={{ display: "flex", alignItems: "flex-end", gap: 12 }}>
          {faceBtn(
            "B", BUTTON.B,
            "linear-gradient(145deg, #c45000 0%, #8b3800 100%)",
            40,
            "0 4px 12px rgba(180,80,0,0.5), inset 0 1px 0 rgba(255,255,255,0.15), 0 0 0 1px #5a2000",
            10
          )}
          {faceBtn(
            "A", BUTTON.A,
            "linear-gradient(145deg, #cc2222 0%, #8b0000 100%)",
            48,
            "0 4px 16px rgba(200,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.15), 0 0 0 1px #5a0000"
          )}
        </div>
      </div>
    </div>
  );
}
