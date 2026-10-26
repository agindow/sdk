# false-positive controls

`agw_stats::control` is a standalone, synchronous local library. It performs no
I/O and makes no guard, policy, repair or delivery decisions. Contract §10.23
specifies its rules. Supplies snapshots, owns routing, and
will connect controls to guarded evaluations. No production qualification or
latency guarantee is claimed.

## Selecting a baseline

Construct a `Scope` for exactly one tenant, vendor, endpoint-identifying operation,
request/response side, tenant segment, volume cohort and JSON Pointer field.
`Window` carries complete-window metadata and caller-owned sample counts.
`select(&current, &history, season)` returns history indices newest first, or
`None` for unavailable evidence. Keep the metadata and detector data in the same
immutable snapshot so the indices cannot select a different generation's data.

For hourly windows with weekly seasonality use `duration: 3600`,
`period: 604800`, and the desired number of prior weeks (1–52). UTC epoch alignment
is deliberate: local-time daylight-saving adjustment is not implemented. Selection
rolls with each current window, requires every configured seasonal slot and never
falls back to another segment or to an old static baseline. Up to 4096 history
windows can be inspected. No summary scores or quantiles are averaged here.
Callers decide how to compare the selected raw windows without losing their
seasonal identity; no statistical significance is inferred from metadata.

## Materiality and ranking

Construct `Floors::new(alpha, agent_floor, mean_floor, overrides)` once per
configuration snapshot. There are no calibrated default values. An override
matches exactly `(tenant, field, signal)`; duplicates are rejected. Floors are
inclusive normalized absolute effects in `(0,1]`, not payload numeric values.
For example, a completeness effect can be an absolute rate delta. A mean effect
requires an explicitly documented domain normalization before entering this API.

`evaluate(tenant, field, evidence)` returns below-materiality, not-significant,
or eligible with a weighted score. `rank` returns eligible evidence indices by
descending score, then agent-signal weight, then original input order. Mean shift
has weight 1 and the five agent-relevant signal types have weight 2. Ranking is
an alert-priority aid; a semantic write finding must still reach policy regardless
of notification rank or budget.

Evidence must carry a valid statistical p-value and the **total** tested hypothesis
count, including tests that produced no candidate. Significance uses Bonferroni
`p <= alpha / tests`; this is not a new p-value estimator. The original
APIs remain descriptive. 's [significance adapters](./SIGNIFICANCE.md) now
compute p-values and normalized effects from the same raw windows, retaining
existing detector-local minimum-evidence gates. Their `Report::decision` method
passes only local findings into. Confidence scores must never be substituted
for p-values; missing significance is unavailable, not `p = 0`. Runtime guard
invocation remains integration work. These controls do not turn
synthetic evidence into calibrated detection.

## Daily notification budget

Keep exactly one `AlertBudget` per `(tenant, connection)` and serialize access.
Do not construct one per field, detector or request. Pass `timestamp / 86400` as
the UTC day to `record`, only after an eligible finding. For a cap of 3:

| Eligible finding in the day | Action | Notifications so far |
|---|---|---|
| 1 | `Individual` | 1 |
| 2 | `Individual` | 2 |
| 3 | `CreateSummary { consolidated: 1 }` | 3 |
| 4 | `UpdateSummary { consolidated: 2 }` | 3 |
| Subsequent | Update the same summary | 3 |

The final slot is reserved for overflow, so consolidation does not exceed the
hard cap. Cap 1 creates the summary immediately; cap 0 disables notifications.
Updates must mutate the existing summary **without sending another notification**.
The host owns delivery, durable identity/state and atomic summary updates. This
library's in-memory counter does not enforce a distributed or restart-safe cap.
A later UTC day resets it; older days are rejected without changing state.

## Offline replay and remaining release evidence

`control::replay::run` accepts exactly 30 consecutive populated UTC `Day`s, a
season configuration, floors, an explicit false-positive threshold and provenance.
Each day supplies actual traffic volume and labeled `Opportunity` evidence. The
last day's traffic must exceed the first day's. Each opportunity includes current
window metadata, local history, supplied significance/effect and an independently
reviewed `actual_drift` label. Each day requires at least one negative opportunity.

This is a control-layer replay harness over precomputed detector evidence, not a
raw HTTP replay runner. Prepare evidence using the actual detectors against the
same selected seasonal windows, including all tested negative opportunities.
Baseline warm-up traffic precedes the measured 30 days. Keep corpus identity,
configuration, significance method and labels together for review.

The runner calls the real selection/materiality functions. It reports positives,
true positives, negatives, false positives and unavailable windows. Rate is false
positives / all negative opportunities, measured **before alert budgets**. It must
be strictly below the caller's threshold; any unavailable selection prevents
production qualification. Runs are bounded to 100,000 opportunities and 10 million
history-slot visits before detector-control work begins.

`Provenance::Synthetic` can never produce `production_qualified: true`.
`Production { dataset_id }` is a caller attestation, not independently verified
proof. Review the corpus provenance and output before treating the flag as release
evidence. Positive counts expose missed detections; the false-positive gate alone
makes no recall guarantee. Synthetic regression fixtures exercise the machinery
but cannot close 's production criterion. No production dataset or agreed
production threshold exists yet; stays Blocked until that replay passes.
```sh
cargo test -p agw-stats --test control_selection --test control_materiality --test control_budget --test control_replay
```