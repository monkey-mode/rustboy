import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "RustBoy",
  description: "Game Boy & NES emulator — Rust core compiled to WebAssembly.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="text-gray-100 min-h-screen antialiased">
        {children}
      </body>
    </html>
  );
}
