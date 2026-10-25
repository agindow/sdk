import { readFileSync } from 'node:fs';
import { cpus, platform, release, arch } from 'node:os';
/** Runtime metadata plus the compiler/flags recorded when this exact guest was built. */
export function performanceEnvironment() {
  const build = /** @type {Record<string, unknown>} */ (JSON.parse(readFileSync(new URL('../dist/guest/build.json', import.meta.url), 'utf8')));
  const value = { node: process.version, v8: process.versions.v8, cpu: cpus()[0]?.model, platform: platform(), os: release(), arch: arch(),
    rust: build['rust'], target: build['target'], profile: build['profile'], flags: build['flags'], wasmBindgen: build['wasmBindgen'] };
  validateEnvironment(value);
  return value;
}
/** @param {unknown} input */
export function validateEnvironment(input) {
  if (input === null || typeof input !== 'object' || Array.isArray(input)) throw Error('missing runtime/compiler metadata');
  const value = /** @type {Record<string, unknown>} */ (input);
  for (const name of ['node','v8','cpu','platform','os','arch','rust','target','profile','wasmBindgen']) {
    const entry = value[name];
    if (typeof entry !== 'string' || entry.length === 0 || entry.length > 512) throw Error(`missing environment ${name}`);
  }
  if (value['target'] !== 'wasm32-unknown-unknown' || value['profile'] !== 'release' || value['wasmBindgen'] !== 'wasm-bindgen 0.2.129'
    || !Array.isArray(value['flags']) || value['flags'].some(flag => typeof flag !== 'string')) throw Error('wrong build target/flags/bindgen');
}
