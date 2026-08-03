import { defineConfig } from "vite";

// Vite config tuned for Tauri:
// - Fixed port 1420 (matches tauri.conf.json devUrl)
// - strictPort: bail if the port is taken, don't silently shift
// - HMR over the same port
// - clearScreen: false so Tauri's Rust compile errors stay visible
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    hmr: {
      protocol: "ws",
      host: "127.0.0.1",
      port: 1421,
    },
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // WebView2 is evergreen, so there's no old-browser floor to hold to.
    target: "es2022",
    // Vite 8 replaced esbuild with oxc as the bundled minifier; naming
    // "esbuild" explicitly now fails the build asking for a separate esbuild
    // install. `true` takes whatever Vite ships, which is the point.
    minify: true,
    sourcemap: false,
  },
});
