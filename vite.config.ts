import { defineConfig } from "vite";

export default defineConfig({
  worker: { format: "es" },
  build: { target: "es2022", rollupOptions: { input: { main: "index.html", rust: "rust.html", legacy: "legacy.html" } } },
});
