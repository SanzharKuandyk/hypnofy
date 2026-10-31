import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  envDir: false,
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/.b/**", "**/src-tauri/**", "**/test-results/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
});
