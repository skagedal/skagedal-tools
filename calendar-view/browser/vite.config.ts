import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `pnpm dev` against a running `calendar-view --serve`: set
// CALENDAR_VIEW_URL to the URL it prints and /api is proxied there.
export default defineConfig({
  plugins: [react()],
  server: process.env.CALENDAR_VIEW_URL
    ? { proxy: { "/api": process.env.CALENDAR_VIEW_URL } }
    : undefined,
});
