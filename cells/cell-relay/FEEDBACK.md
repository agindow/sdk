# Structured agent feedback

`agw_guard::feedback::Feedback::from_outcome(&outcome)` borrows a bounded
call outcome and returns optional agent feedback. It does not execute, retry,
throw, publish an audit artifact, or replace the original upstream outcome.
See [runtime contract §7 and §10.35](/agindow-runtime-contract.md) and
.

For a halt, send `feedback.http_status` (409) and serialize `feedback.payload`.
Strict mode uses `drift_halted`; repair-mode semantic/unknown write halts use
`drift_blocked`. The separately exposed `ErrorCode` pins the eight §7 HTTP error
codes, including `internal` (500 as representative of 5xx, which clients bypass).

Example payload for a strict request halt with confirmed rename evidence:
```json
{
  "error": "drift_halted",
  "tool": "stripe.create_customer",
  "drift_class": "shape",
  "drift_id": "aud_example",
  "hint": "Confirmed field rename `source` → `payment_method`. Update the call to use `payment_method` and validate it against the authoritative schema. The upstream call was not dispatched.",
  "audit": "https://api.agindow.com/v1/audit/aud_example"
}
```
For `drift_passthrough`, `http_status` is `None`: attach the payload as additive
notification metadata and preserve upstream HTTP status/body. Do not turn this
into an HTTP error response. Clean, observed, repaired and availability-bypassed
calls return no feedback. Complete malformed write passthrough remains a drift
notification even when its audit verdict is `bypassed`.

Hints name local admitted field paths. Confirmed shape identity permits rename
instructions; missing expected-form evidence asks for authoritative schema
information. Semantic/unknown hints require verification/human review and never
authorize automatic conversions. Mixed reports retain the strongest class.
Diagnostic paths are bounded and escaped; values and invented dates are absent.
Response-side feedback warns that the call was dispatched and may have completed:
hosts retain that outcome and verify it and idempotency before any replay.
`Display` uses the same hint as the JSON payload.

The audit URL encodes the audit ID as one path segment. It identifies the returned
audit draft; this library does not make the retrieval service available.
Composes statistical diagnosis evidence into the audit artifact; this
adapter consumes outcomes and does not wire the statistical guard or SDKs.

## Measuring recovery

Use `feedback::recovery::Tracker` off-path, with one instance per tenant, arm and
measurement window. Register an actual block using its tool and audit ID. Report
explicit retry outcomes against that original identity. Only
`RetryOutcome::VerifiedTaskSuccess` counts as recovery: the host must verify the
task result, not infer success from HTTP 200, a repair, or a bypass.

`snapshot` returns serializable counts of distinct blocked, retried and
recovered calls. `recovery_rate` returns recovered/retried or `None` if no retry
has occurred. Multiple failed/bypassed retries count once in the denominator;
a later verified success counts once in the numerator. Duplicate/late callbacks
cannot inflate counts or erase success. Separate tracker instances can compare
structured and unstructured feedback arms without pooling their populations.

Capacity is explicit (1–65,536 entries); tool/audit identities are nonempty and
at most 512 bytes each. Full windows reject new blocks without eviction, retaining
finalized identities for deduplication. Unknown blocks, invalid identities and
capacity rejection are visible errors for the host's telemetry pipeline, never
call-path policy failures. Hosts export snapshots and rotate windows explicitly.
No persistence, retry ceiling/backoff or hosted telemetry is installed.

The local blocked/corrected-call test measures 1/1 verified recovery in a
controlled fixture. That validates accounting, not agent effectiveness. Production
recovery rates and a causal improvement over unstructured feedback remain unmeasured.

## Verification
```sh
cargo test -p agw-guard --test feedback --test feedback_edges --test feedback_recovery
```
Tests pin the taxonomy, mode/class/operation matrix, availability skips, JSON
round-trip, human-readable hints, request dispatch, response outcome retention,
malformed feedback, mixed semantic safety, URL escaping and bounded deduplication.

Delivery and qualification follow-ups are tracked explicitly: SDK/HTTP feedback
wiring belongs to,
,
,
And
.
Owns production recovery
measurement and qualification.

's [explicit replay adapter](./IDEMPOTENCY.md) retains this feedback with the
original audit identity and adds a terminal-key note for writes. Reusing a blocked
key returns the same block, even after correcting arguments. An authorized new
logical write can be correlated to the original known-undispatched block for
recovery measurement; uncertain completed writes require reconciliation first.
