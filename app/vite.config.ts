import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "src") },
    // A single web3.js instance across the app + linked SDK, so `instanceof
    // PublicKey` holds across the boundary.
    dedupe: ["@solana/web3.js", "@solana/spl-token", "react", "react-dom"],
  },
  define: {
    // Some wallet-adapter deps reference process.env / global. Keep NODE_ENV
    // real (the more specific key wins) so prod branches aren't silently lost.
    "process.env.NODE_ENV": JSON.stringify(mode === "development" ? "development" : "production"),
    "process.env": {},
    global: "globalThis",
  },
  server: { port: 5173 },
}));
