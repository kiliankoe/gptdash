import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/socket.io": {
        target: process.env.VITE_API_URL || "http://localhost:8080",
        changeOrigin: true,
        ws: true,
      },
    },
  },
});
