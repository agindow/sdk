import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const cli = process.env['AGINDOW_WORKER_BUILD'] ?? 'worker-build';
const version = execFileSync(cli, ['--version'], { encoding: 'utf8' }).trim();
if (!version.endsWith('0.7.5')) throw Error('worker-build 0.7.5 is required');
execFileSync(cli, [fileURLToPath(new URL('../crates/http-host', import.meta.url)), '--release', '--no-opt', '--',
  '--no-default-features', '--features', 'worker-host', '--locked'], { stdio: 'inherit', env: process.env });
