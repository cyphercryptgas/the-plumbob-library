import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Fixed port + strictPort match what the Tauri shell (src-tauri, plateau 3)
// will expect from its devUrl; clearScreen off keeps Rust output visible when
// both run together.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist" }
});
