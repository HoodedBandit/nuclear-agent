import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [react()],
  base: "/dashboard-assets/",
  build: {
    manifest: true,
    outDir: resolve(__dirname, "../../crates/agent-daemon/static-modern"),
    emptyOutDir: true,
    sourcemap: false,
    target: "es2021"
  },
  test: {
    environment: "jsdom",
    setupFiles: [resolve(__dirname, "src/test/setup.ts")]
  }
});
