import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "src") },
    // A single web3.js instance across the app + linked SDK, so `instanceof
    // PublicKey` holds across the boundary.
    dedupe: ["@solana/web3.js", "react", "react-dom"],
  },
  define: {
    // Some wallet-adapter deps reference process.env / global.
    "process.env": {},
    global: "globalThis",
  },
  server: { port: 5173 },
});
