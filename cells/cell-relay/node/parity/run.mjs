import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runParity } from './compare.mjs';
const python = fileURLToPath(new URL('../../python/', import.meta.url));
const definition = JSON.parse(readFileSync(new URL('./cases.json', import.meta.url), 'utf8'));
runParity(definition,
  [process.execPath, fileURLToPath(new URL('./typescript.mjs', import.meta.url))],
  ['uv', 'run', '--project', python, '--frozen', 'python', `${python}/scripts/parity.py`]);
console.log(`Python/TypeScript parity: ${definition.cases.length} shared fixtures passed`);
