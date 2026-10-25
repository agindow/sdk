import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
export default defineConfig({ test: { include: [fileURLToPath(new URL('./tests/**/*.test.ts', import.meta.url))] } });
