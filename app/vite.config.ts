import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives this dev server; the port is fixed so tauri.conf.json can
// point at it. No remote origins are configured anywhere (ARCHITECTURE.md §14).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", sourcemap: true },
});
