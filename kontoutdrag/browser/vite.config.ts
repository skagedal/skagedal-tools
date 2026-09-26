import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `pnpm dev` against a running `kontoutdrag view --serve`: set
// KONTOUTDRAG_URL to the URL it prints and /api is proxied there.
export default defineConfig({
  plugins: [react()],
  server: process.env.KONTOUTDRAG_URL
    ? { proxy: { "/api": process.env.KONTOUTDRAG_URL } }
    : undefined,
});
