import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdtempSync, rmSync, mkdirSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
const cli = new URL('../dist/cli.cjs', import.meta.url).pathname;
/** @param {string} directory */
function hash(directory) {
  return createHash('sha256').update(readdirSync(directory).sort().map(name => `${name}:${readFileSync(join(directory, name)).toString('hex')}`).join('\n')).digest('hex');
}
test('packaged report is offline/read-only, skips malformed source and uses the generated banner', () => {
  const directory = mkdtempSync(join(tmpdir(), 'agw-cli-'));
  try {
    const poison = join(directory, 'no-network.cjs');
    writeFileSync(poison, `for (const module of ['node:http','node:https','node:net','node:tls','node:dns']) { const item=require(module); for (const name of ['request','get','connect','createConnection','lookup','resolve']) if(typeof item[name]==='function') item[name]=()=>{throw Error('network forbidden')}; } globalThis.fetch=()=>{throw Error('network forbidden')};`);
    writeFileSync(join(directory, 'tools.ts'), `import { agindow } from 'agindow'; const tools = { run: { name: 'fixture.run', execute: () => ({ok:true}) } }; agindow.wrap(tools); throw Error('do not execute');`);
    writeFileSync(join(directory, 'broken.ts'), 'const =');
    const before = hash(directory);
    const output = execFileSync(process.execPath, ['--require', poison, cli, 'report', directory], { encoding: 'utf8', env: { PATH: process.env.PATH }, cwd: directory });
    assert.match(output, /Tools detected: 1/); assert.match(output, /Already wrapped: 1/); assert.match(output, /Skipped files: 1/);
    assert.match(output, /No registry snapshot/); assert.equal(hash(directory), before);
    const banner = readFileSync(new URL('../../../../brand/terminal/agindow.txt', import.meta.url), 'utf8');
    assert.ok(output.startsWith(banner));
    assert.match(readFileSync(new URL('../dist/THIRD-PARTY.txt', import.meta.url), 'utf8'), /acorn-typescript/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
test('packaged scan handles actions and syncs idempotently, refreshes opt-out and typechecks its generated TS', () => {
  const directory = mkdtempSync(join(tmpdir(), 'agw-codegen-'));
  try {
    const source = (/** @type {string} */ builder) => `import { ${builder} } from 'nango';\nexport default ${builder}({ exec: async (nango, input) => { const response = await nango.post<{status:string}>({endpoint:'/v1/customers',data:input}); return response.data; } });\n`;
    const input = JSON.parse(readFileSync(new URL('../examples/nango-contracts.json', import.meta.url), 'utf8'));
    const original = input.contracts[0];
    input.contracts.push({ ...original, file: 'stripe/syncs/sync-customer.ts' });
    for (const [path, builder] of [[original.file, 'createAction'], [input.contracts[1].file, 'createSync']]) {
      const parent = join(directory, path.substring(0, path.lastIndexOf('/'))); mkdirSync(parent, { recursive: true });
      writeFileSync(join(directory, path), source(builder));
    }
    const snapshot = join(directory, 'snapshot.json'); writeFileSync(snapshot, JSON.stringify(input));
    const run = () => execFileSync(process.execPath, [cli, 'scan', directory, '--snapshot', snapshot], { encoding: 'utf8' });
    assert.match(run(), /Changed files: 2/);
    const first = readFileSync(join(directory, original.file), 'utf8');
    assert.match(run(), /Changed files: 0/); assert.equal(readFileSync(join(directory, original.file), 'utf8'), first);
    execFileSync(process.execPath, [cli, 'scan', directory, '--snapshot', snapshot], { env: { ...process.env, AGINDOW_DISABLED: 'true' } });
    assert.match(readFileSync(join(directory, original.file), 'utf8'), /"disabled":true/);
    const definition = join(directory, 'nango.d.ts');
    writeFileSync(definition, `declare module 'nango' { interface Context { ActionError: new(payload:unknown)=>unknown; post<T>(request:{endpoint:string;data:unknown}):Promise<{data:T}> } export function createAction<R>(definition:{exec:(nango:Context,input:{source:string})=>Promise<R>}):unknown; export const createSync: typeof createAction; }`);
    execFileSync(process.execPath, [new URL('../../../../node_modules/typescript/bin/tsc', import.meta.url).pathname,
      '--strict', '--noEmit', '--target', 'ES2022', '--module', 'ESNext', '--moduleResolution', 'bundler', definition,
      join(directory, original.file), join(directory, input.contracts[1].file)], { stdio: 'inherit', cwd: directory });
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
