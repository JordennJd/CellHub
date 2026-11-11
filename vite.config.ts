import { defineConfig } from "vite";

const isolationHeaders = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
};

export default defineConfig({
  worker: { format: "es" },
  build: { target: "es2022" },
  server: {
    headers: isolationHeaders,
    proxy: {
      "/ws": { target: "ws://127.0.0.1:8082", ws: true },
    },
  },
  preview: { headers: isolationHeaders },
});
