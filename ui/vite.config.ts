import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The daemon serves this build as static assets under /assets and
// index.html at /, so paths must stay relative to that root.
export default defineConfig({
  plugins: [react()],
  base: "/",
  build: {
    outDir: "dist",
    assetsDir: "assets",
  },
});
