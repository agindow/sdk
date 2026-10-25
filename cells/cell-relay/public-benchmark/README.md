# Public conditional safety benchmark — protocol v1

**Release candidate; publication is gated on private held-out custody and two or
three external validator signatures.** The checked-in baseline reports are local
reproduction artifacts, not externally validated launch scores. No Agindow arm is
included. This directory has no dependency on the Agindow engine, a service,
account, database or hosted runner. Node >=22 and TypeScript are enough to build;
the compiled runner uses only Node built-ins.

This implements and
the runtime contract.
It extends the internal eval's scoring concepts, without importing its cases,
results, engine or thresholds. The internal eval has 42 synthetic cases and
fixture-supplied findings. This public development set has 280 synthetic cases:
seven classes × ten value/context variants × four proposal scenarios. These are
parameterized coverage, not 280 independent real incidents or a production
accuracy estimate. Independent corpus review and expansion remain necessary.

## What is measured

The `conditional-safety-v1` track measures **repair and policy safety given
supplied findings**. Every agent gets the same original JSON write and a finding
(source, target, proposed value, confidence), or no finding. It does not measure
detection, learning, latency, real vendor behavior, or end-to-end agent success.
A future detection-plus-repair track must have a different version, input
protocol and reports; supplied findings cannot stand in for detector accuracy.

`runtime` tools operate on the write in flight. `repo-and-pr` tools modify source
and produce a build or patch; their adapters must perform that work themselves
and project the resulting write into this conditional track. These solve
different problems. Reports declare one category; the ranking function rejects
mixed categories. No combined ranking is meaningful. A repo adapter's write
projection does not establish PR correctness, deployment success or availability.

Every case has separately authored migration intent (the complete expected
write), a vendor acceptance rule, preserved caller memo/context, synthetic origin
and rationale. Repairable scenarios have correct high-confidence proposals;
misleading scenarios have wrong high-confidence proposals; ambiguous scenarios
have correct low-confidence proposals; unsupported scenarios have no proposal.
Labels describe supplied evidence, not the capabilities of any implementation.
The whole-write oracle credits correct output regardless of proposal scenario.
The fixture vendor checks target presence/type (and an optional enum allowlist),
independently of intended values. Accepted wrong intent or lost preserved fields
is an invariant violation; a rejected write is not an executed unsafe write.
This simplified fixture vendor is not vendor integration qualification.

All cases remain in both overall denominators. Exactly one outcome is recorded:
whole-write recovery, accepted invariant violation, explicit block, or upstream
rejection. Required metrics are recovery/all cases and violations/all cases,
with counts, dispatches, violations/dispatch (null if no dispatch), and per-class
and per-scenario slices. Invalid output, crashes, timeouts, duplicate, unknown or
missing case IDs fail evaluation; they are never silently credited as blocks.
`validateReport` re-scores stored writes rather than trusting submitted metrics.

Within each category and identical corpus, rank by **fewer violations first,
then greater recovery**. Equal metric pairs are performance ties; entry names
only stabilize display order. Thus over-correction loses to a clean arm even
when the latter recovers fewer cases. Always blocking has no recovery and ties
other clean zero-recovery arms; it cannot win through hidden denominators.
Reports must always show both metrics, never a recovery-only headline.

## Reproduce from public artifacts

From the repository root:

```sh
pnpm install --frozen-lockfile
pnpm exec tsc -p cells/cell-relay/public-benchmark/tsconfig.json
node cells/cell-relay/public-benchmark/dist/cli.js baselines cells/cell-relay/public-benchmark/development.v1.json > /tmp/baselines.json
cmp /tmp/baselines.json cells/cell-relay/public-benchmark/baselines.v1.json
pnpm exec vitest run tests/unit/public-benchmark*.test.ts
```

Alternatively compile the TS files with a standalone TypeScript installation and
Node types using the same strict settings. The source runner requires no runtime
npm dependencies. The two baseline arms preserve the entire original object:
`no-repair` returns it untouched; `naive-full-payload` reconstructs it and applies
any supplied proposal without confidence or intent gating. Neither gets labels
or the oracle. The naive arm is deliberately ungated, not deliberately destructive.

Reports are canonical JSON with lexically sorted object keys, original case
order, no timestamps, host paths or randomness. Corpus identity pins canonical
case bytes via SHA-256; the release digest pins exact public source, methodology,
corpus and baseline bytes. A semantic change requires a new protocol/corpus
version and fresh commitments and signatures; never compare incompatible sets.

## Run your own agents

An executable receives one JSON object on stdin per fresh process:

```json
{"id":"case-id","payload":{"value":1},"finding":null}
```

It must emit one JSON object on stdout, with the identical ID and `write` equal
to its resulting JSON object, or null for an explicit block. Logging belongs on
stderr. Scenarios, classes, expected writes and labels are excluded from agent
input; case IDs are public and may be used for development, so development scores
are never official held-out scores. Do not supply development-case lookup agents
as generalization evidence. The runner bounds each process to ten seconds and
2 MiB output, invokes it without a shell, and fails the run on invalid output.
Your executable runs with your local privileges; this runner is not a sandbox.

```sh
node cells/cell-relay/public-benchmark/dist/cli.js run cells/cell-relay/public-benchmark/development.v1.json my-agent runtime -- node /absolute/path/to/agent.cjs > /tmp/my-report.json
```

`repo-and-pr` replaces `runtime` for a repository tool adapter. Precomputed
submissions can use `score CORPUS SUBMISSION`, where SUBMISSION is a JSON object
with `entry`, `category` and `results` (an array of the per-case stdout objects).
Keep results from different tracks and categories in separate reports.

## Synthetic admission and sealed held-out custody

Only fixtures authored without tenant or private evidence are eligible in either
split. No anonymization, paraphrasing, aggregation, or synthetic regeneration of
private observations qualifies. `validateCorpus` rejects other provenance/origin
labels. Those declarations are auditable authorship claims, not a programmatic
proof of origin: the custodian and validators must independently review sources
and attest synthetic-only authorship. Do not import the private drift corpus.

No sealed cases, seeds, identity list, generator output, or oracle may be committed
to the public repository. A **custodian** runs this command on a private machine,
with the sealed path on private storage outside the checkout:

```sh
node cells/cell-relay/public-benchmark/dist/cli.js seal cells/cell-relay/public-benchmark/development.v1.json /private/storage/heldout-v1.json heldout-v1 /tmp/heldout-commitment.json
node cells/cell-relay/public-benchmark/dist/cli.js verify-heldout cells/cell-relay/public-benchmark/development.v1.json /private/storage/heldout-v1.json /tmp/heldout-commitment.json
```

`seal` uses cryptographic randomness for fixture values, creates private bytes
with mode 0600 and exclusive writes, checks disjoint IDs **and** payloads, and
outputs a separate commitment containing **only version and SHA-256 of exact
sealed bytes**. The random seed is not persisted or published. Only the commitment
is eligible for public version control. Retain private bytes and backups: a hash
cannot recover a lost set. Publish the commitment before receiving official
entries; never rotate a set to improve a result. Validators privately verify the
same sealed bytes without releasing them. Reusing templates limits hidden-set
diversity; secret values alone do not establish resistance to benchmark gaming.
No official held-out score is produced by this development runner.

## Validator sign-off and publication gate

Before publication, the owner recruits two or three independent external
validators. Each reviews methodology, synthetic provenance and held-out custody,
compiles the public harness, reproduces **both** checked-in baseline reports from
public artifacts alone, and verifies the private commitment. The publication
owner independently verifies affiliation and obtains each Ed25519 public key
through a trusted channel. Never trust identities/keys supplied inside a report.
Test keys in the unit suite are not validators or sign-offs.

With commitment and reproduced baseline report array available:

```sh
node cells/cell-relay/public-benchmark/dist/cli.js digest
node cells/cell-relay/public-benchmark/dist/cli.js statement cells/cell-relay/public-benchmark/development.v1.json /tmp/baselines.json /tmp/heldout-commitment.json > /tmp/statement.json
```

Each validator signs the **exact statement bytes**, Ed25519, and returns a
base64 signature. The statement binds methodology/source digest, exact sealed
commitment, and each canonical baseline report hash. `statement` has no trailing
newline. Any source, methodology or baseline edit invalidates previous sign-off.
Trust roots are a separately reviewed JSON array of `{id, publicKey}` (PEM);
attestations are an array of `{validator, signature}`. The release command checks
2–3 distinct trusted identities and keys, all signatures, both reproduced baseline
arms, complete metrics and corpus identity:

```sh
node cells/cell-relay/public-benchmark/dist/cli.js release cells/cell-relay/public-benchmark/development.v1.json /tmp/baselines.json /tmp/heldout-commitment.json /tmp/attestations.json /tmp/trusted-validators.json > /tmp/launch.json
```

Publish only a successfully generated launch artifact with its reviewed trust
roots and public artifacts. Do not publish an empty board or call local baseline
reports validator-run scores. An Agindow entry or any substituted arm is refused.
This command prepares an artifact; it never publishes or contacts validators.
**Current release status: pending custodian commitment and real external
attestations.** Those human actions are not replaced by passing local tests.
