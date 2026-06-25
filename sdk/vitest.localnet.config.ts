import { defineConfig } from "vitest/config";

// Localnet parity suite — loads the program .so into an in-process bankrun
// runtime. Local-only (not run in CI): needs `cargo build-sbf` output under
// ../target/deploy. Run with `npm run test:localnet`.
export default defineConfig({
  test: {
    include: ["localnet/**/*.test.ts"],
    testTimeout: 120_000,
    hookTimeout: 120_000,
  },
});
