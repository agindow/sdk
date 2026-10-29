/** Private TII-164 browser projection. This validates shape/linkage, never execution authority.
 * The host must admit original SDK, confined-validation and provider receipts independently. */
function object(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw Error('Invalid playground evidence');
  return value as Record<string, unknown>;
}
function text(value: unknown, max = 1024): string {
  if (typeof value !== 'string' || !value || value.length > max || /[\u0000-\u001f\u007f]/u.test(value)) throw Error('Invalid text');
  return value;
}
export function playgroundId(value: unknown): string {
  const id = text(value, 128);
  if (!/^[-a-zA-Z0-9_:.~]+$/u.test(id)) throw Error('Invalid identifier');
  return id;
}
function digest(value: unknown): string {
  const result = text(value, 64);
  if (!/^[a-f0-9]{64}$/u.test(result)) throw Error('Invalid digest');
  return result;
}
function sha(value: unknown): string {
  const result = text(value, 64);
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/u.test(result)) throw Error('Invalid commit');
  return result;
}
function integer(value: unknown, min = 0, max = Number.MAX_SAFE_INTEGER): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < min || value > max) throw Error('Invalid bound');
  return value;
}
function flag(value: unknown): boolean {
  if (typeof value !== 'boolean') throw Error('Invalid flag');
  return value;
}
function choice<T extends string>(value: unknown, choices: readonly T[]): T {
  const result = choices.find(choice => choice === value);
  if (!result) throw Error('Invalid state');
  return result;
}
function rows<T>(value: unknown, max: number, read: (row: Record<string, unknown>) => T): T[] {
  if (!Array.isArray(value) || value.length > max) throw Error('Evidence limit');
  return value.map(row => read(object(row)));
}
function body(value: unknown, max = 65536): string {
  if (typeof value !== 'string' || new TextEncoder().encode(value).length > max) throw Error('Disclosure limit');
  return value;
}
function instant(value: unknown): string {
  const result = text(value, 32);
  if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{1,3})?Z$/u.test(result) || !Number.isFinite(Date.parse(result))) throw Error('Invalid timestamp');
  return result;
}
function windowOf(input: unknown) {
  const v = object(input);
  const startedAt = instant(v.startedAt), endedAt = v.endedAt === null ? null : instant(v.endedAt), complete = flag(v.complete);
  if (complete && endedAt === null || endedAt !== null && Date.parse(endedAt) < Date.parse(startedAt)) throw Error('Invalid capture lifetime');
  return { id: playgroundId(v.id), digest: digest(v.digest), count: integer(v.count, 0, 256), complete, startedAt, endedAt };
}
function captureOf(input: unknown) {
  const v = object(input);
  const result = {
    id: playgroundId(v.id), requestScope: digest(v.requestScope), guestDigest: digest(v.guestDigest),
    baseline: windowOf(v.baseline), current: windowOf(v.current),
    state: choice(v.state, ['findings', 'stable', 'insufficient', 'failed', 'bypassed']),
    findings: rows(v.findings, 32, f => {
      const incident = f.incident === undefined || f.incident === null ? null : text(f.incident, 128);
      if (incident !== null && !/^[-a-zA-Z0-9_:.]+$/u.test(incident)) throw Error('Invalid incident');
      return { id: playgroundId(f.id), summary: text(f.summary), incident };
    }),
  };
  if (result.baseline.endedAt !== null && Date.parse(result.baseline.endedAt) > Date.parse(result.current.startedAt)) throw Error('Overlapping capture windows');
  if (result.state === 'stable' && (!result.baseline.complete || !result.current.complete || !result.baseline.count || !result.current.count)) throw Error('Incomplete stable comparison');
  if (result.state === 'findings' && (!result.findings.length || !result.baseline.complete || !result.current.complete
    || !result.baseline.count || !result.current.count)) throw Error('Incomplete finding');
  if (result.state !== 'findings' && result.findings.length) throw Error('Forced finding');
  if (new Set(result.findings.map(f => f.id)).size !== result.findings.length) throw Error('Duplicate finding');
  return result;
}
function runtimeOf(input: unknown) {
  const v = object(input);
  const result = {
    id: playgroundId(v.id), capture: playgroundId(v.capture), finding: playgroundId(v.finding),
    sdkDigest: digest(v.sdkDigest), factsDigest: digest(v.factsDigest),
    mode: choice(v.mode, ['repair', 'strict', 'observe']),
    decision: choice(v.decision, ['applied', 'proposed', 'observe', 'bypassed', 'refused', 'blocked', 'failed']),
    verifier: choice(v.verifier, ['passed', 'refused', 'fault', 'unavailable']),
    original: body(v.original), patched: v.patched === null ? null : body(v.patched),
    changes: rows(v.changes, 16, c => ({ from: text(c.from, 64), to: text(c.to, 64) })),
    outcome: choice(v.outcome, ['completed', 'rejected', 'failed', 'uncertain', 'not-dispatched']),
    upstreamStatus: v.upstreamStatus === null ? null : integer(v.upstreamStatus, 100, 599),
  };
  if (result.decision === 'applied' && (result.verifier !== 'passed' || result.mode === 'observe'
    || result.patched === null || result.patched === result.original || result.changes.length !== 1)) throw Error('Unverified applied repair');
  if (result.outcome === 'completed' && (result.upstreamStatus === null || result.upstreamStatus < 200 || result.upstreamStatus >= 300)) throw Error('Unconfirmed upstream outcome');
  return result;
}
function sourceOf(input: unknown) {
  const v = object(input), validation = object(v.validation);
  if (v.kind !== 'deterministic' || !body(v.diff, 131072).trim()) throw Error('Unsupported source candidate');
  return { kind: 'deterministic' as const,
    id: playgroundId(v.id), runtime: playgroundId(v.runtime), finding: playgroundId(v.finding), factsDigest: digest(v.factsDigest),
    repository: playgroundId(v.repository), base: sha(v.base), patchDigest: digest(v.patchDigest), diff: body(v.diff, 131072),
    preservation: choice(v.preservation, ['passed', 'failed', 'inconclusive']),
    validation: {
      digest: digest(validation.digest), state: choice(validation.state, ['passed', 'failed', 'inconclusive', 'skipped']),
      checks: rows(validation.checks, 128, c => ({ id: playgroundId(c.id),
        before: choice(c.before, ['passed', 'failed', 'inconclusive', 'skipped']),
        after: choice(c.after, ['passed', 'failed', 'inconclusive', 'skipped']) })),
    },
  };
}
function publicationOf(input: unknown, repositoryName: string) {
  const v = object(input);
  if (v.kind !== 'draft-created') throw Error('Not a provider draft receipt');
  const number = integer(v.number, 1), url = text(v.url, 2048);
  // This first profile supports github.com only. Enterprise/provider support requires explicit host review.
  if (!/^[a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+$/u.test(repositoryName)
    || url !== `https://github.com/${repositoryName}/pull/${number}`) throw Error('Wrong provider destination');
  return {
    kind: 'draft-created' as const, id: playgroundId(v.id), source: playgroundId(v.source), repository: playgroundId(v.repository),
    base: sha(v.base), head: sha(v.head), patchDigest: digest(v.patchDigest), validationDigest: digest(v.validationDigest), url, number,
    ci: choice(v.ci, ['unknown', 'pending', 'passed', 'failed', 'approval-required']),
    lifecycle: choice(v.lifecycle, ['open', 'closed', 'merged', 'unknown']),
  };
}
function validated(source: ReturnType<typeof sourceOf>): boolean {
  return source.preservation === 'passed' && source.validation.state === 'passed'
    && source.validation.checks.some(c => c.before === 'failed' && c.after === 'passed')
    && source.validation.checks.every(c => c.after === 'passed');
}
function runOf(input: unknown, repository: { id: string; name: string; base: string }) {
  const v = object(input);
  const capture = v.capture === null ? null : captureOf(v.capture);
  const runtime = v.runtime === null ? null : runtimeOf(v.runtime);
  const source = v.source === null ? null : sourceOf(v.source);
  const publication = v.publication === null ? null : publicationOf(v.publication, repository.name);
  if (runtime && (!capture || capture.state !== 'findings' || runtime.capture !== capture.id
    || !capture.findings.some(f => f.id === runtime.finding))) throw Error('Wrong runtime evidence');
  if (source && (!runtime || source.runtime !== runtime.id || source.finding !== runtime.finding
    || source.factsDigest !== runtime.factsDigest || source.repository !== repository.id || source.base !== repository.base)) throw Error('Wrong source evidence');
  if (publication && (!source || !validated(source) || publication.source !== source.id || publication.repository !== source.repository
    || publication.base !== source.base || publication.patchDigest !== source.patchDigest
    || publication.validationDigest !== source.validation.digest)) throw Error('Wrong publication evidence');
  return { id: playgroundId(v.id), state: choice(v.state, ['pending', 'running', 'stopped', 'complete']),
    evidence: choice(v.evidence, ['actual-api', 'local-test']), capture, runtime, source, publication };
}
export interface PlaygroundScope { readonly organization: string; readonly profile: string }
export function readPlaygroundReview(input: unknown, scope: PlaygroundScope) {
  if (new TextEncoder().encode(JSON.stringify(input)).length > 400000) throw Error('Review limit');
  const v = object(input), op = object(v.operation), repo = object(v.repository), limits = object(v.limits), ports = object(v.services);
  if (v.organization !== scope.organization || v.profile !== scope.profile || v.environment !== 'test' || op.nonProduction !== true) throw Error('Wrong Test scope');
  const expiresAt = text(v.expiresAt, 32);
  if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{1,3})?Z$/u.test(expiresAt) || !Number.isFinite(Date.parse(expiresAt))) throw Error('Invalid expiry');
  const repository = { id: playgroundId(repo.id), name: text(repo.name, 256), base: sha(repo.base), branch: text(repo.branch, 128) };
  const maxCalls = integer(limits.maxCalls, 1, 257);
  const run = v.run === null ? null : runOf(v.run, repository);
  if (run?.capture && run.capture.baseline.count + run.capture.current.count
    + (run.runtime && run.runtime.outcome !== 'not-dispatched' ? 1 : 0) > maxCalls) throw Error('Run call limit');
  return {
    organization: playgroundId(scope.organization), environment: 'test' as const, profile: playgroundId(scope.profile),
    role: choice(v.role, ['owner', 'admin', 'member', 'viewer']), reviewDigest: digest(v.reviewDigest), revision: integer(v.revision), expiresAt,
    operation: { id: playgroundId(op.id), label: text(op.label), nonProduction: true as const }, repository,
    limits: { maxCalls, deadlineMs: integer(limits.deadlineMs, 1, 300000), retentionMs: integer(limits.retentionMs, 1, 86400000) },
    runtimeConfigured: flag(v.runtimeConfigured), automaticPublication: flag(v.automaticPublication),
    services: { capture: flag(ports.capture), runtime: flag(ports.runtime), generation: flag(ports.generation), validation: flag(ports.validation), publication: flag(ports.publication) },
    unavailable: rows(v.unavailable, 8, r => ({ step: choice(r.step, ['capture', 'runtime', 'generation', 'validation', 'publication', 'permission']), reason: text(r.reason) })),
    run,
  };
}
export type PlaygroundReview = ReturnType<typeof readPlaygroundReview>;
/** Never trust a host's overall status as a substitute for each independently admitted receipt. */
export function playgroundOutcome(review: PlaygroundReview): 'incomplete' | 'local-test' | 'recorded' {
  const run = review.run;
  if (!run || run.state !== 'complete' || run.capture?.state !== 'findings' || run.runtime?.decision !== 'applied'
    || run.runtime.outcome !== 'completed' || !run.source || !validated(run.source) || !run.publication) return 'incomplete';
  return run.evidence === 'local-test' ? 'local-test' : 'recorded';
}
export function readPlaygroundCatalog(input: unknown): readonly { id: string; label: string }[] {
  return rows(object(input).profiles, 32, v => ({ id: playgroundId(v.id), label: text(v.label, 256) }));
}
