import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const cwd = fileURLToPath(new URL('../', import.meta.url));
for (const [mutation, path] of [
  ['fencing', 'owner-faults'], ['uniqueness', 'owner-retention'],
  ['retention', 'owner-retention'], ['quarantine', 'owner-retention'],
]) {
  const result = spawnSync(process.execPath, ['--test', `hosted/tests/${path}.test.mjs`], {
    cwd, encoding: 'utf8', env: { ...process.env, AGINDOW_OWNER_MUTATION: mutation },
  });
  if (result.error || result.signal || result.status !== 1 || !result.stdout.includes('ERR_ASSERTION') ||
    result.stdout.includes('mutation target changed')) {
    throw Error(`owner mutation ${mutation} did not fail a behavioral assertion`);
  }
  console.log(`owner mutation rejected: ${mutation}`);
}
