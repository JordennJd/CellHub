import { defineConfig } from "vite";

export default defineConfig({
  worker: { format: "es" },
  build: { target: "es2022" },
  server: {
    proxy: {
      "/ws": { target: "ws://127.0.0.1:8082", ws: true },
    },
  },
});
