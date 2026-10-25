import { randomBytes } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { agentInput, canonical, development, hash, object, validateCorpus } from './corpus.js';
import { baselines, evaluate, validateReport } from './scorer.js';
import { commitHeldout, release, releaseStatement, validateCommitment, verifyHeldout } from './protocol.js';
import type { Attestation, Corpus, Report, Validator } from './types.js';

const root = fileURLToPath(new URL('../', import.meta.url));
const repo = resolve(root, '../../..');
function arg(args: string[], n: number): string {
  const value = args[n];
  if (!value) throw new Error('Missing argument; see public-benchmark/README.md');
  return value;
}
function read(path: string): unknown { return JSON.parse(readFileSync(path, 'utf8')) as unknown; }
function corpusAt(path: string): Corpus {
  const value = read(path);
  validateCorpus(value);
  return value;
}
function output(value: object): void { process.stdout.write(canonical(value) + '\n'); }
function save(path: string, value: object): void { writeFileSync(path, canonical(value) + '\n', { flag: 'wx', mode: 0o600 }); }
const artifacts = ['README.md', 'types.ts', 'corpus.ts', 'scorer.ts', 'protocol.ts', 'cli.ts', 'tsconfig.json', 'development.v1.json', 'baselines.v1.json'];
function artifactDigest(): string {
  return hash(canonical(Object.fromEntries(artifacts.map(file => [file, hash(readFileSync(resolve(root, file), 'utf8'))]))));
}
function reportArray(path: string, corpus: Corpus): Report[] {
  const value = read(path);
  if (!Array.isArray(value)) throw new Error('Expected report array');
  return value.map((item: unknown) => { validateReport(item, corpus); return item; });
}
function validatorsAt(path: string): Validator[] {
  const value = read(path);
  if (!Array.isArray(value)) throw new Error('Expected out-of-band validator trust roots');
  return value.map((v: unknown) => {
    if (!object(v) || typeof v.id !== 'string' || typeof v.publicKey !== 'string') throw new Error('Invalid validator');
    return { id: v.id, publicKey: v.publicKey };
  });
}
function attestationsAt(path: string): Attestation[] {
  const value = read(path);
  if (!Array.isArray(value)) throw new Error('Expected attestations');
  return value.map((a: unknown) => {
    if (!object(a) || typeof a.validator !== 'string' || typeof a.signature !== 'string') throw new Error('Invalid attestation');
    return { validator: a.validator, signature: a.signature };
  });
}
function main(args: string[]): void {
  const command = arg(args, 0);
  if (command === 'generate') { save(arg(args, 1), development()); return; }
  if (command === 'digest') { output({ sha256: artifactDigest() }); return; }
  const corpus = corpusAt(arg(args, 1));
  if (command === 'baselines') { output(baselines(corpus)); return; }
  if (command === 'run') {
    if (corpus.split !== 'development') throw new Error('Public runner accepts only development cases');
    const entry = arg(args, 2);
    const category = arg(args, 3);
    if (category !== 'runtime' && category !== 'repo-and-pr') throw new Error('Declare runtime or repo-and-pr');
    if (arg(args, 4) !== '--') throw new Error('Separate agent command with --');
    const executable = arg(args, 5);
    const results: unknown[] = [];
    for (const c of corpus.cases) {
      const child = spawnSync(executable, args.slice(6), { input: canonical(agentInput(c)), encoding: 'utf8', timeout: 10000, maxBuffer: 2 * 1024 * 1024 });
      if (child.error || child.status !== 0) throw new Error(`Agent failed on ${c.id}; no score produced`);
      const result: unknown = JSON.parse(child.stdout);
      // One process per case prevents one result from being silently attributed to another.
      if (!object(result) || result.id !== c.id) throw new Error(`Invalid agent result for ${c.id}`);
      results.push(result);
    }
    output(evaluate(corpus, entry, category, results));
    return;
  }
  if (command === 'score') {
    const submission = read(arg(args, 2));
    if (!object(submission) || typeof submission.entry !== 'string' || (submission.category !== 'runtime' && submission.category !== 'repo-and-pr')) throw new Error('Invalid submission');
    output(evaluate(corpus, submission.entry, submission.category, submission.results));
    return;
  }
  if (command === 'seal') {
    const destination = resolve(arg(args, 2));
    const actual = resolve(realpathSync(dirname(destination)), relative(dirname(destination), destination));
    const rel = relative(repo, actual);
    if (!rel.startsWith('..' + '/') && !isAbsolute(rel)) throw new Error('Sealed cases must live outside the repository');
    const sealed = development(arg(args, 3), 'heldout', randomBytes(32).toString('hex'));
    const bytes = canonical(sealed) + '\n';
    const commitment = commitHeldout(bytes, corpus);
    writeFileSync(destination, bytes, { flag: 'wx', mode: 0o600 });
    save(arg(args, 4), commitment);
    return;
  }
  if (command === 'verify-heldout') {
    const commitment = read(arg(args, 3));
    validateCommitment(commitment);
    verifyHeldout(readFileSync(arg(args, 2), 'utf8'), commitment, corpus);
    output({ verified: true });
    return;
  }
  if (command === 'statement' || command === 'release') {
    const reports = reportArray(arg(args, 2), corpus);
    const commitment = read(arg(args, 3));
    validateCommitment(commitment);
    const digest = artifactDigest();
    if (command === 'statement') { process.stdout.write(releaseStatement(corpus, reports, commitment, digest)); return; }
    output(release(corpus, reports, commitment, attestationsAt(arg(args, 4)), digest, validatorsAt(arg(args, 5))));
    return;
  }
  throw new Error('Unknown command; see public-benchmark/README.md');
}
try { main(process.argv.slice(2)); } catch (error) {
  process.stderr.write(`benchmark: ${error instanceof Error ? error.message : 'invalid input'}\n`);
  process.exitCode = 1;
}
