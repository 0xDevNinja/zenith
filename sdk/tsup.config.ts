import { defineConfig } from "tsup";

// Dual ESM + CJS output with type declarations.
export default defineConfig({
  entry: ["src/index.ts"],
  format: ["esm", "cjs"],
  dts: true,
  sourcemap: true,
  clean: true,
  treeshake: true,
});
