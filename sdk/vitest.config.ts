import { defineConfig } from "vitest/config";

// Default suite (runs in CI): pure unit/parity tests only. The localnet
// bankrun test lives under localnet/ and is run separately via
// `npm run test:localnet` (needs the built program .so, kept out of CI).
export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
  },
});
