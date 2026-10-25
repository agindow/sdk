import { defineConfig } from 'vitest/config';
export default defineConfig({
  test: { include: ['wasm/tests/**/*.test.ts'] },
});
