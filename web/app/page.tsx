import dynamic from "next/dynamic";

const Emulator = dynamic(() => import("@/components/Emulator"), { ssr: false });

export default function Home() {
  return (
    <div className="h-screen flex flex-col overflow-hidden select-none">

      {/* ── Header ── */}
      <header style={{
        flexShrink: 0,
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "0 24px", height: 48,
        borderBottom: "1px solid #111",
        background: "rgba(4,8,12,0.85)",
        backdropFilter: "blur(12px)",
        position: "relative",
        zIndex: 10,
      }}>
        {/* Left: brand */}
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          {/* Logo mark */}
          <div style={{
            width: 26, height: 26, borderRadius: 6, flexShrink: 0,
            background: "linear-gradient(135deg, #16a34a 0%, #4ade80 100%)",
            display: "flex", alignItems: "center", justifyContent: "center",
            fontSize: 13, fontWeight: 900, color: "#000",
            boxShadow: "0 0 14px rgba(74,222,128,0.35), inset 0 1px 0 rgba(255,255,255,0.3)",
          }}>R</div>

          {/* Brand name */}
          <div style={{ display: "flex", flexDirection: "column", lineHeight: 1, gap: 1 }}>
            <span style={{
              fontWeight: 900, fontSize: 15, letterSpacing: "0.06em",
              background: "linear-gradient(90deg, #4ade80 0%, #86efac 100%)",
              WebkitBackgroundClip: "text", WebkitTextFillColor: "transparent",
              filter: "drop-shadow(0 0 8px rgba(74,222,128,0.4))",
            }}>RUSTBOY</span>
            <span style={{
              fontSize: 8, color: "#1a3a20", letterSpacing: "0.3em",
              textTransform: "uppercase", fontFamily: "monospace",
            }}>Multi-System Emulator</span>
          </div>
        </div>

        {/* Center: decorative dots */}
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          {[0.4, 0.7, 1, 0.7, 0.4].map((o, i) => (
            <div key={i} style={{
              width: 3, height: 3, borderRadius: "50%",
              background: `rgba(74,222,128,${o * 0.25})`,
            }} />
          ))}
        </div>

        {/* Right: tech label */}
        <span style={{
          fontSize: 9, color: "#1a2a1a", letterSpacing: "0.2em",
          textTransform: "uppercase", fontFamily: "monospace",
        }}>Rust + WebAssembly</span>
      </header>

      {/* ── Main ── */}
      <main className="flex-1 flex items-center justify-center overflow-hidden p-4">
        <Emulator />
      </main>
    </div>
  );
}
