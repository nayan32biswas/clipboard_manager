import { defineConfig } from "vitest/config";

export default defineConfig({
  // Keep Rust compiler errors visible in the terminal.
  clearScreen: false,
  // Tauri expects this exact port (see devUrl in tauri.conf.json).
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
