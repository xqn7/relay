import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: {
    target: "es2022",
    cssCodeSplit: false,
    assetsInlineLimit: 0,
    chunkSizeWarningLimit: 450,
    // The main window, and the mini player / desktop lyrics windows.
    rollupOptions: { input: { main: "index.html", overlay: "overlay.html" } },
  },
  test: { include: ["src/**/*.test.ts"] },
});
