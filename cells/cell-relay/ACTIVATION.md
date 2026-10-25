# Local invariant activation (TI-54)

`agw_invariants::activation` implements runtime contract §10.33. It is an explicit
background library; constructing a guard still selects `NullEngine`. It does not
schedule jobs, fetch profiles, run an LLM, authorize repair or change call modes.

A `Controller` owns one exact tenant/connection/vendor/operation/contract scope.
`Candidate` carries immutable local DSL definitions/bindings and optional TI-23
paired-endpoint definitions. At most 128 local and 16 paired rules are admitted. Authorization and pending
customer overrides obey the same DSL bounds before they are retained.
Configured capacity (1–1024) separately bounds receipts, candidate versions and
decisions. One additional rejected receipt is retained as fault evidence. Full
buffers reject work; nothing evicts adverse evidence. Conflicting duplicates and
receipt-buffer exhaustion permanently freeze promotion in that controller instance
and invalidate pending publications. Explicit customer reconciliation still works;
the host must resolve the evidence fault outside the request path.

The trusted administrative host performs complete independent replay outside
calls and supplies `Receipt`s. They retain dataset IDs, independence/completeness
attestations, policy/candidate versions, stage, timestamps and raw counts. This
is an explicit trust boundary: the library cannot verify dataset provenance or
independence from a receipt. Training and holdout IDs must differ; holdouts cannot
be reused across stages or overlap training IDs. An unavailable evaluation lowers
coverage. Known-invalid samples measure missed violations, not repair success.

`Policy` has no threshold defaults. Customers declare minimum valid/invalid sample
counts, coverage floors and false-positive/missed-violation ceilings in parts per
million. Comparisons use inclusive integer cross-products. Each independent window
and their aggregate must meet the rate ceilings: a good large window cannot dilute
a bad small one. Historical adverse, incomplete or expired receipts for the same
candidate continue to veto promotion after policy revisions or version-only relabeling/reordering. Identical receipt IDs
are idempotent; changed contents conflict. Candidate versions cannot change contents.
A new version must restart shadow qualification; older versions cannot replace a
newer active candidate. Customer pins prevent automatic version changes.

Authorization declares allowed families, candidate version interval, stage and
call-mode permissions and expiry. Non-enum local rules need exact-definition
business/relationship attestations. Paired rules additionally retain full endpoint,
concept, unit, tolerance, freshness/skew definitions and explicit business-revision
semantics. Profiles never attest those meanings. Shadow precedes reads, which
precedes blocking writes. Next-stage replay starts after the prior publication.
A blocking stage additionally requires a candidate-specific independent-production
calibration attestation, dataset ID, sampling assumptions and measured counts.

No production qualification dataset is supplied. The tests model host attestations
and deterministic counts; they do not establish production thresholds, error rates
or safe blocking-write activation.

## Publication and host integration

1. Create a controller with the trusted scope and explicit bounds. Configure a
   scoped policy and customer controls; absent authorization leaves candidates inactive.
2. Run replay in a background worker and append all receipts, including negative,
   incomplete and expired windows. Use a monotonically advancing host clock.
3. `prepare(candidate, stage, now)` records a decision and compiles a complete
   immutable replacement. Inspect `Prepared::decision()` for promote/reject/defer
   reasons, complete authorization, controls, counts and retained evidence.
4. Publish through `publish_with(pending, now, |generation| ...)`, calling
   `Engine::publish_activation(side, generation)` in the callback and mapping its
   error. The engine must already be explicitly configured as statistical. Publication rechecks expiry at commit, and next-stage replay starts after the
   actual publication time. Its publisher validates the whole replacement before one Arc swap, retaining all
   unrelated statistical/shape selections and the readiness breaker. A default
   engine or existing explicit customer selection refuses replacement.
5. For customer edits, call `configure` then `reconcile_with` with the same atomic
   publisher. Local overrides replace exact IDs; disables also remove paired
   rules. Unsupported paired overrides require an explicitly selected customer
   plan, which automatic publication cannot overwrite. Pins to unavailable versions
   fail without a fallback. Revocation requires a higher revision to authorize again.

Preparation records controller revisions. Any new receipt, customer edit or prior
publication invalidates pending work. A foreign controller's prepared plan is
rejected. Publisher callbacks must preserve their own state on errors; the supplied
native publisher does so. Controllers and engine publication use exclusive
administrative ownership. Calls own Arc plans, never a controller reference/lock;
already-begun calls keep their prior plan, mode and qualification evidence.

A stalled replay, failed compilation, rejected publication or unavailable controller
leaves guarded calls using the prior plan. Expiry, revoked authorization and new
negative evidence freeze escalation and record rejection/requalification, retaining
known semantic protection. An enforcing generation cannot be automatically replaced
by a new shadow candidate or a policy that removes its authorized modes; that
requires an explicit customer plan change. This first policy authorizes no autonomous rollback to
weaker enforcement. Customer-directed disables/overrides are explicit changes.

## Runtime and evidence

Shadow evaluates and audits while reporting only. Reads applies the existing policy
to reads; writes and unknown operations remain report-only for that learned set.
Blocking applies the existing policy matrix, within the authorized modes. Other
findings and explicitly selected rules keep their policy, including external findings
with a coincidentally identical name. Actual call mode remains unchanged.

Each published generation retains its decision, receipts, authorization and controls.
`Controller::decisions()` exposes rejected decisions and publication/conflict outcomes.
Native per-phase audit metadata includes generation, stage, policy/candidate versions
and complete receipt counts, including negative counts. Existing invariant and
paired-endpoint metadata retains scope, actual/expected values, controls and units.
`agindow_activation` identifies the phase-local generation, falling back to the
paired generation if only paired activation supplies evidence on that phase.
Evaluated paired rules also retain their full qualification evidence under
`agindow_cross_endpoint.activation`. These source-specific records preserve both
generations when local and paired selections differ, including paired findings
on the opposite phase from the original publication.
Byte identity and `write_integrity: false` remain explicit. Final audit delivery and
hosted signed distribution/persistence remain separate work.

## Verification

```bash
cargo test -p agw-invariants --test activation --test activation_evidence
cargo test -p agw-guard --test activation --test activation_cross
cargo bench -p agw-guard --bench invariants
node cells/cell-relay/perf/run-availability.mjs
```

The native invariant benchmark measures its fixed integrated fixture, not background
replay or production activation throughput. The availability gate is separate.
