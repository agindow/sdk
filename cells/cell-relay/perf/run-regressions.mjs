import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { cpus, platform, arch } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compare, readMeans } from './check-regressions.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const output = join(root, 'target/native-regressions');
const plan = /** @type {{revision: string, suites: [string, string][]}} */ (
  JSON.parse(readFileSync(new URL('./baseline.v1.json', import.meta.url), 'utf8')));
if (!/^[a-f0-9]{40}$/.test(plan.revision)) throw new Error('baseline must pin a full commit');
rmSync(output, { recursive: true, force: true });
mkdirSync(output, { recursive: true });
/** @param {string} command @param {string[]} args @param {string} [cwd] */
function capture(command, args, cwd = root) {
  return execFileSync(command, args, { cwd, encoding: 'utf8' }).trim();
}
const baseline = mkdtempSync(join(output, 'source-'));
const archive = join(output, 'baseline.tar');
// Export immutable committed sources without mutating the caller's Git metadata.
execFileSync('git', ['archive', '--format=tar', `--output=${archive}`, plan.revision], { cwd: root });
execFileSync('tar', ['-xf', archive, '-C', baseline]);
rmSync(archive);
try {
  writeFileSync(join(output, 'environment.json'), JSON.stringify({
    baselineRevision: plan.revision, candidateRevision: capture('git', ['rev-parse', 'HEAD']),
    candidateDirty: capture('git', ['status', '--porcelain']).length > 0, timestamp: new Date().toISOString(),
    platform: platform(), arch: arch(), cpus: cpus().map(cpu => cpu.model),
    rustc: capture('rustc', ['-vV']), cargo: capture('cargo', ['--version']),
    suites: plan.suites, warmupSeconds: 1, measurementSeconds: 3, samples: 100,
    statistic: 'mean point estimate', threshold: 1.10,
  }, null, 2));
  for (const [label, source] of [['baseline', baseline], ['candidate', root]]) {
    if (label === undefined || source === undefined) throw new Error('missing cohort');
    const target = join(output, label);
    for (const [pkg, bench] of plan.suites) {
      execFileSync('cargo', ['bench', '--locked', '-p', pkg, '--bench', bench, '--',
        '--noplot', '--warm-up-time', '1', '--measurement-time', '3', '--sample-size', '100'], {
        cwd: source, env: { ...process.env, CARGO_TARGET_DIR: target }, stdio: 'inherit',
      });
    }
  }
  const report = compare(readMeans(join(output, 'baseline/criterion')), readMeans(join(output, 'candidate/criterion')));
  writeFileSync(join(output, 'report.json'), JSON.stringify(report, null, 2));
  const summary = `Native Criterion: ${report.passed ? 'PASS' : 'FAIL'} (>10% regression fails)\n` +
    report.rows.map(row => `${row.name}: ${row.changePercent.toFixed(2)}% ${row.passed ? 'PASS' : 'FAIL'}`).join('\n');
  console.log(summary);
  if (process.env['GITHUB_STEP_SUMMARY']) writeFileSync(process.env['GITHUB_STEP_SUMMARY'], summary);
  if (!report.passed) process.exitCode = 1;
} finally {
  rmSync(baseline, { recursive: true, force: true });
}
