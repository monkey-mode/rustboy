import dynamic from "next/dynamic";

// The emulator uses WebAssembly and the Web Audio API; must be client-side only.
const Emulator = dynamic(() => import("@/components/Emulator"), { ssr: false });

export default function Home() {
  return (
    <main className="flex flex-col items-center justify-center min-h-screen gap-6 p-6">
      <h1 className="text-4xl font-bold tracking-tight text-green-400">
        RustBoy
      </h1>
      <p className="text-gray-400 text-sm">
        Game Boy &amp; NES emulator — Rust core compiled to WebAssembly
      </p>
      <Emulator />
    </main>
  );
}
