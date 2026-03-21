export type System = "GB" | "NES" | null;

export const systemAccent = (system: System) =>
  system === "NES" ? "#ff4444" : "#44ff88";

export const systemGlow = (system: System) =>
  system === "NES" ? "rgba(255,68,68,0.15)" : "rgba(68,255,136,0.12)";
