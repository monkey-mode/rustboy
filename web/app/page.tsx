import dynamic from "next/dynamic";

const Emulator = dynamic(() => import("@/components/Emulator"), { ssr: false });

export default function Home() {
  return (
    <div className="h-screen flex flex-col overflow-hidden select-none">
      {/* Header */}
      <header className="flex-none flex items-center justify-between px-6 h-12
        border-b border-gray-800/60 bg-black/30 backdrop-blur-sm">
        <div className="flex items-center gap-2.5">
          <div className="w-6 h-6 rounded bg-green-500/20 border border-green-500/40
            flex items-center justify-center text-green-400 text-xs font-bold">R</div>
          <span className="font-bold tracking-tight bg-gradient-to-r from-green-300 to-green-500
            bg-clip-text text-transparent">RustBoy</span>
          <span className="text-gray-600 text-xs hidden sm:block">
            · Game Boy &amp; NES emulator
          </span>
        </div>
        <span className="text-gray-700 text-xs">Rust + WebAssembly</span>
      </header>

      {/* Main */}
      <main className="flex-1 flex items-center justify-center overflow-hidden p-4">
        <Emulator />
      </main>
    </div>
  );
}
