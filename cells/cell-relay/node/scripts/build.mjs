import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync, readFileSync, rmSync, chmodSync } from 'node:fs';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { stageLegalFiles } from './legal.mjs';
import { notices } from './notices.mjs';
const directory = fileURLToPath(new URL('..', import.meta.url));
const root = resolve(directory, '../../..');
/** @param {string} cmd @param {string[]} args @param {string} [cwd] */
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, { cwd, stdio: 'inherit' });
/** @param {string} path */
const sha256 = path => createHash('sha256').update(readFileSync(path)).digest('hex');
rmSync(resolve(directory, 'dist'), { recursive: true, force: true });
run('cargo', ['build', '-p', 'agw-node-guest', '--release', '--target', 'wasm32-unknown-unknown', '--locked']);
// Honour CARGO_TARGET_DIR (and cargo's configured target dir) instead of
// assuming `<repo>/target` — clean-clone verification and some CI sandboxes
// redirect artifacts outside the tree.
const metadata = /** @type {{ target_directory?: unknown }} */ (JSON.parse(
  execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1', '--manifest-path', resolve(root, 'Cargo.toml')], {
    cwd: root,
    encoding: 'utf8',
  }),
));
if (typeof metadata.target_directory !== 'string' || metadata.target_directory.length === 0) {
  throw new Error('cargo metadata omitted target_directory');
}
const releaseWasm = resolve(metadata.target_directory, 'wasm32-unknown-unknown/release/agw_node_guest.wasm');
const guest = resolve(directory, 'dist/guest');
const guestWeb = resolve(directory, 'dist/guest-web');
mkdirSync(guest, { recursive: true });
mkdirSync(guestWeb, { recursive: true });
const bindgen = process.env.AGINDOW_WASM_BINDGEN ?? 'wasm-bindgen';
run(bindgen, [releaseWasm, '--target', 'nodejs', '--out-dir', guest]);
// Same release .wasm bytes; web glue only — no second guest crate (TII-109).
run(bindgen, [releaseWasm, '--target', 'web', '--out-dir', guestWeb]);
const nodeWasm = resolve(guest, 'agw_node_guest_bg.wasm');
const webWasm = resolve(guestWeb, 'agw_node_guest_bg.wasm');
const nodeDigest = sha256(nodeWasm);
const webDigest = sha256(webWasm);
if (nodeDigest !== webDigest) {
  throw new Error(`guest-web WASM drifted from node guest (${nodeDigest} vs ${webDigest})`);
}
writeFileSync(resolve(guest, 'package.json'), '{"type":"commonjs"}\n');
writeFileSync(resolve(guestWeb, 'package.json'), '{"type":"module"}\n');
const buildMeta = {
  rust: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
  target: 'wasm32-unknown-unknown', profile: 'release',
  flags: [process.env['RUSTFLAGS'] ?? '', process.env['CARGO_ENCODED_RUSTFLAGS'] ?? ''],
  wasmBindgen: execFileSync(bindgen, ['--version'], { encoding: 'utf8' }).trim(),
  wasmSha256: nodeDigest,
};
writeFileSync(resolve(guest, 'build.json'), JSON.stringify(buildMeta) + '\n');
writeFileSync(resolve(guestWeb, 'build.json'), JSON.stringify({ ...buildMeta, bindgenTarget: 'web' }) + '\n');
run('pnpm', ['exec', 'tsc', '-p', resolve(directory, 'tsconfig.json')]);
run('pnpm', ['exec', 'tsc', '-p', resolve(directory, 'tsconfig.json'), '--module', 'commonjs',
  '--verbatimModuleSyntax', 'false', '--outDir', resolve(directory, 'dist/cjs')]);
writeFileSync(resolve(directory, 'dist/cjs/package.json'), '{"type":"commonjs"}\n');
for (const format of ['esm', 'cjs']) for (const module of ['wasm', 'otel']) {
  const file = resolve(directory, `dist/${format}/${module}.js`);
  writeFileSync(file, readFileSync(file, 'utf8').replace('__AGINDOW_FILENAME__',
    format === 'esm' ? 'fileURLToPath(import.meta.url)' : '__filename'));
}
// Bundle the frozen schema validator so the independently packed SDK never
// imports the monorepo's source-only contracts package at runtime.
const auditInputs = new Set();
for (const [directoryName, format] of /** @type {const} */ ([['esm', 'esm'], ['cjs', 'cjs']])) {
  const auditBundle = await build({ entryPoints: [resolve(directory, 'src/audit.ts')], absWorkingDir: root, metafile: true,
    outfile: resolve(directory, `dist/${directoryName}/audit.js`),
    bundle: true, platform: 'node', target: 'node22', format, legalComments: 'inline' });
  for (const input of Object.keys(auditBundle.metafile.inputs)) auditInputs.add(input);
}
stageLegalFiles(root, directory);
// The brand is optional packaging content: removing brand/ never breaks SDKs.
let banner = '';
try { if (process.env.AGINDOW_BRANDING !== 'off') banner = readFileSync(resolve(root, 'brand/terminal/agindow.txt'), 'utf8'); } catch { /* OEM build */ }
const cli = await build({ entryPoints: [resolve(directory, 'cli/main.ts')], outfile: resolve(directory, 'dist/cli.cjs'),
  absWorkingDir: root, bundle: true, platform: 'node', target: 'node22', format: 'cjs', legalComments: 'linked',
  plugins: [{ name: 'typed-sandbox-source', setup(builder) {
    builder.onResolve({ filter: /\?raw$/ }, args => ({ path: resolve(dirname(args.importer), args.path.slice(0, -4)), namespace: 'raw' }));
    builder.onLoad({ filter: /.*/, namespace: 'raw' }, args => ({ contents: readFileSync(args.path, 'utf8'), loader: 'text' }));
  } }],
  metafile: true, banner: { js: '#!/usr/bin/env node' }, define: { __AGINDOW_BANNER__: JSON.stringify(banner) } });
writeFileSync(resolve(directory, 'dist/THIRD-PARTY.txt'), notices([...Object.keys(cli.metafile.inputs), ...auditInputs], root));
chmodSync(resolve(directory, 'dist/cli.cjs'), 0o755);
