import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "RustBoy – Game Boy Emulator",
  description: "A Game Boy emulator written in Rust and compiled to WebAssembly.",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className="bg-gray-950 text-gray-100 min-h-screen antialiased">
        {children}
      </body>
    </html>
  );
}
