# TII-74 guarded qualification — profile v3

Internal synthetic qualification of [TI-24's explicit native guard](../../../../../../../tasks/tickets/tier-1/EPIC-06/TI-24.md),
under [contract §10.2](../../../../../../../agindow-runtime-contract.md).
The separate [v2 conditional seam](../README.md) remains a diagnostic and is embedded
as `seam` in each v3 report. These profiles exercise different evidence boundaries;
their results are not a like-for-like accuracy improvement.

```bash
cargo test -p agw-repair --test guarded_eval --locked
cargo run -p agw-repair --example guarded-eval --locked -- \
  cells/cell-relay/crates/repair/examples/drift_eval/guarded/baseline.v3.json \
  target/drift-eval/guarded-current.json
```

The command writes evidence before comparison, including failed comparisons.
Expected panic probes use Rust's ordinary panic hook, so the CLI prints injected
panic messages even though the guard contains them and the command succeeds.

## Evidence boundary

All 42 case identities and independent scoring labels are retained. Both native
arms share the same supplied findings/candidates. Guarded execution calls real
`gated::Call` with complete bounded capture, repair/write policy and a local
`Pipeline`; the response is a clean empty object. Only bytes captured in the
actual dispatch callback are scored. The boxed upstream outcome and original
request slice are checked by identity on availability bypass.

[verification.v1.json](../verification.v1.json) is a separately authored caller
fixture contract, loaded before execution. It declares numeric scaling, explicit
source/value migration mappings or rename identity, and preservation of every
other caller field. Missing authority is explicit for ambiguous/unsupported
cases. TI-66 allows only an explicit empty-string-to-null mapping. Mapping facts do
not bypass the numeric fence on the other three repairable type transitions. The verifier is given only
original input, facts, patched bytes and applied finding indices. It receives no
proposal, confidence, gold expected write, label, rationale or vendor schema.
The numeric check is a fixture constraint on existing numeric values, not a
numeric coercion implementation or production money validator.

Normal verification returns false for absent/contradictory authority, mismatched
source or unrelated context changes, even for high-confidence schema-valid
proposals. Existing policy handles the unresolved finding. The verifier is not
an oracle: 27 healthy cases never invoke it, and four invoke it and refuse.
Neither schema validity nor refusal can count as recovery.

Each record retains exact facts, verifier input bytes and applied indices,
invocation/result trace, decision, dispatch count/bytes, bypass count, original
identity checks and independently recomputed whole-write scoring. Outcome metrics
cover every class and label, retaining both all-case and per-dispatch violation
rates. Mandatory verification metrics show invoked/passed/refused/faulted/not-run,
bypasses and dispatches, separately for healthy and fault cohorts.

## Measured results

| Profile/cohort | Cases | Repairable recovery | Verified | Blocks | Rejections | Unsafe writes | Dispatches | Bypasses |
|---|---|---|---|---|---|---|---|---|
| v2 conditional selective seam (TI-66) | 42 | 11/14 | No independent verifier | 21 | 6 | 4 | 21 | n/a |
| v3 healthy guarded (TI-66) | 42 | 11/14 | 11 passed, 4 refused | 25 | 6 | 0 | 17 | 0 |
| Forced normal refusal | 42 | 0/14 | 15 refused | 34 | 8 | 0 | 8 | 0 |
| Verifier error | 42 | 0/14 | 15 faulted | 21 | 8 | 13 | 21 | 15 |
| Verifier panic | 42 | 0/14 | 15 faulted | 21 | 8 | 13 | 21 | 15 |
| Gate not ready | 42 | 0/14 | Not invoked | 0 | 12 | 30 | 42 | 42 |
| Verifier work exhausted | 42 | 0/14 | 15 faulted | 21 | 8 | 13 | 21 | 15 |

The original ten recoveries remain, and `null_semantics-2` now recovers through
independent verification after TI-66. The remaining three misses are numeric type
transitions, still counted in the 14-case repairable denominator. All
four wrong overconfident semantic proposals complete verification normally with
missing authority, return false and halt without dispatch. Paired tests hold
payload/confidence constant while removing or contradicting independent facts.
Context corruption cannot earn approval or safe whole-write recovery.

The 210 fault probes are separate, equal-weight synthetic probes across five
fault modes, not traffic rates. Their aggregate is 69 unsafe writes / 113
dispatches, with 87 once-per-call bypasses; it is never folded into healthy safety
counts. Availability favors original execution even when the original write is
unsafe. This reproduces the existing availability contract, not a missing
runtime protection. TI-66's newly attempted repair adds one verifier error, panic and budget
bypass: 66/110 becomes 69/113 unsafe fault dispatches. This increased forced-fault
exposure is retained, not masked as a safety improvement. Thresholds and the
availability policy did not change.

## Why overall recovery is low

The frozen corpus deliberately contains 14 repairable, 14 ambiguous and 14
unsupported cases. Twenty-eight have no unique authoritative mapping. Of the
14 repairable cases, three require a forbidden numeric type transition. Thus
11/42 overall (11/14 independently repairable) is the available justified result
with these inputs and the current numeric policy. We do not exclude the three
numeric misses to claim perfect repairability or fabricate facts for the other 28.

The CLI now writes `guarded-current.diagnosis.json` beside the report and prints
case causes. It combines actual guard verification outcomes with a repeat of the
pure native patch assessment offline; diagnostics cannot authorize a repair or
change scores. Current primary causes reconcile all 42 cases:

| Cause | Count |
|---|---:|
| Recovered | 11 |
| Numeric coercion fence (three repairable, four ambiguous) | 7 |
| Confidence below threshold | 4 |
| Unconfirmed rename identity | 2 |
| No proposal / missing mapping | 14 |
| Independent verification refused | 4 |

The original committed v3 report remains the 10/14 TII-74 qualification baseline.
The current engine passes against it with the same case bytes, labels and facts.
Empty-string normalization requires the explicit semantic finding, ≥0.99
confidence and actual empty source; nonempty strings are never cleared. Numeric
rules, all other type transitions and unknown-evidence decisions remain intact.

## Regression boundary and validation

The v3 gate rescoring uses dispatch bytes, not claimed success. It requires the
entire evidence contract, records, verification/outcome metrics and v2 diagnostic
report. Comparable healthy cases may not lose recovery or gain unsafe writes;
blanket blocking fails recovery. Facts/corpus/profile/version changes require an
explicit migration. V2 cannot deserialize as a v3 report. Derived floating rates
allow only the existing 1e-15 JSON-roundtrip tolerance; counts remain exact.

CI retains the v2 job and independently compares v3 with its committed baseline,
PR-base baseline and last successful trusted main report. The v3 cache namespace
is separate. Only a successful main push promotes it. Initial absence of a v3
PR-base/cache is reported explicitly; v2 is never substituted.

Mutation checks caught bypassed verification, confidence-only approval,
proposal-like self-authorization when authority is absent, ignored context, and
an always-refusing verifier. Report mutations also catch missing evidence,
changed facts, inconsistent bytes/scores/metrics and profile/version reuse.

These fixtures qualify supplied findings and local caller facts only. They do
not qualify detectors, generic semantic intent verification, production accuracy,
SDK/default-guest or hosted coverage. Production confidence calibration remains
open; the public benchmark is owned by TII-21a/TII-21b.
