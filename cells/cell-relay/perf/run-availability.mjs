import { execFileSync } from 'node:child_process';
import { cpus, arch, platform, release, totalmem } from 'node:os';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { checkAvailability } from './check-availability.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const manifest = join(root, 'Cargo.toml');
const raw = /** @type {unknown} */ (JSON.parse(execFileSync('cargo',
  ['metadata', '--no-deps', '--format-version', '1', '--manifest-path', manifest], { encoding: 'utf8' })));
if (raw === null || typeof raw !== 'object' || !('target_directory' in raw) || typeof raw.target_directory !== 'string') {
  throw new Error('availability-gate: cargo metadata omitted target_directory');
}
const reports = join(raw.target_directory, 'criterion', 'availability');
// Reject stale results: a failed/partial benchmark must never pass on old data.
rmSync(reports, { recursive: true, force: true });
mkdirSync(reports, { recursive: true });
writeFileSync(join(reports, 'environment.json'), JSON.stringify({
  timestamp: new Date().toISOString(), platform: platform(), os: release(), arch: arch(),
  cpuModels: [...new Set(cpus().map(cpu => cpu.model))], logicalCpus: cpus().length,
  memoryBytes: totalmem(), node: process.version,
  rustc: execFileSync('rustc', ['-vV'], { encoding: 'utf8' }),
  cargo: execFileSync('cargo', ['--version'], { encoding: 'utf8' }),
  profile: 'bench (release)', samples: 100, warmupSeconds: 1, measurementSeconds: 3,
  confidence: 0.95, resamples: 10000, statistic: 'mean upper confidence bound',
  originalOperation: 'identity return and branch validation included; setup and I/O excluded',
}, null, 2));
execFileSync('cargo', ['bench', '--manifest-path', manifest, '-p', 'agw-guard',
  '--bench', 'availability', '--', '--noplot'], { cwd: root, stdio: 'inherit' });
for (const line of checkAvailability(reports)) console.log(line);
