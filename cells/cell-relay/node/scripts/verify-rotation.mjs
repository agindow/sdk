/** TI-64: offline CLI/determinism/exit-code and 1M-event memory qualification.
 * Transpile only the report and contract sources; no native guest is needed. */
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, readFileSync, mkdirSync, symlinkSync, rmSync, openSync, writeSync, closeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../../../../', import.meta.url));
const temp = mkdtempSync(join(tmpdir(), 'agw-rotation-'));
try {
  const output = join(temp, 'runtime'), config = join(temp, 'tsconfig.json');
  writeFileSync(config, JSON.stringify({ extends: resolve(root, 'tsconfig.base.json'), compilerOptions: {
    noEmit:false, outDir:output, rootDir:root, module:'commonjs', verbatimModuleSyntax:false,
  }, include:[resolve(root,'contracts/src/**/*.ts'),resolve(root,'cells/cell-relay/node/cli/rotation/**/*.ts')] }));
  execFileSync('pnpm',['exec','tsc','-p',config],{cwd:root,stdio:'pipe'});
  writeFileSync(join(output,'package.json'),'{"type":"commonjs"}');
  mkdirSync(join(output,'node_modules'));
  symlinkSync(join(output,'contracts'),join(output,'node_modules/contracts'),'dir');
  writeFileSync(join(output,'contracts/package.json'),'{"type":"commonjs","main":"src/index.js"}');
  for (const dependency of ['ajv','ajv-formats']) symlinkSync(resolve(root,'node_modules',dependency),join(output,'node_modules',dependency),'dir');
  const cli = join(output,'cli.cjs');
  writeFileSync(cli,"require('./cells/cell-relay/node/cli/rotation/command.js').rotationCommand(process.argv.slice(2)).then(code=>process.exitCode=code).catch(()=>{console.error('AGINDOW: report unavailable');process.exitCode=1});");
  const eventFile=join(temp,'events.ndjson'), factsFile=join(temp,'facts.ndjson');
  const line=readFileSync(resolve(root,'contracts/examples/credential_event.valid.json'),'utf8').replace(/\s+/gu,' ');
  writeFileSync(eventFile,`${line}\n`);writeFileSync(factsFile,'');
  const args=['--max-old-space-size=64',cli,eventFile,factsFile,'--as-of','2026-10-22T12:00:00Z','--json'];
  const run=()=>spawnSync(process.execPath,args,{encoding:'utf8',timeout:60000});
  const first=run(),second=run();
  assert.equal(first.status,0);assert.equal(second.stdout,first.stdout);
  const required=spawnSync(process.execPath,[...args,'--require=verifiable'],{encoding:'utf8'});
  assert.equal(required.status,1);
  writeFileSync(eventFile,`${line}\ninvalid\n`);
  const malformed=run();assert.equal(malformed.status,1);assert.equal(malformed.stdout,'');assert.ok(malformed.stderr.length<256);
  const fd=openSync(eventFile,'w');
  const seed=/** @type {Record<string,unknown>} */(JSON.parse(line));
  try {
    for(let block=0;block<1000;block++) {
      let chunk='';
      for(let offset=0;offset<1000;offset++) chunk+=`${JSON.stringify({...seed,id:`evt-${block*1000+offset}`})}\n`;
      writeSync(fd,chunk);
    }
  } finally {closeSync(fd);}
  const expected=first.stdout.replace('"health":"degrading"','"health":"failing"');
  assert.notEqual(expected,first.stdout);
  const large=run();assert.equal(large.status,0,large.stderr);assert.equal(large.stdout,expected);
  console.log('PASS: deterministic CLI, exit codes, no partial output, 1,000,000 events under 64 MiB heap');
} finally { rmSync(temp,{recursive:true,force:true}); }
