# Strict-mode human review

The explicit bounded-capture guard halts unresolved drift on reads, writes and
unknown operations in strict mode. Only independently verified repairs at
confidence ≥0.99 proceed. Availability faults still bypass once. The statistical
guest is default; this review path remains an opt-in native library.

After execution, `review::case(&outcome, &scope)` constructs a frozen
Envelope. Scope binds the
tenant, connection, vendor, operation, selected contract version and caller's
repair identity. The case uses the original blocked audit ID/time, includes
request/response decision reports, proposed operations, exact replacement-token
bytes as hex, assessment confidence/status and verification outcomes. Original
body bytes are never copied into review evidence. Proposal tokens and audit
metadata can contain customer data; the host owns access and retention.

`review::Outbox::new(tenant, capacity, byte_limit)` provides a caller-owned FIFO.
`submit` returns false for an exact pending duplicate or an outcome without a
strict halt; conflicting IDs fail. `front` remains until `ack(id)` confirms
host delivery. Tenant/scope errors, saturation and oversized evidence return
explicit errors while the policy halt and original upstream outcome remain
intact. Publication belongs outside the inspection path; it sends no network
request and never depends synchronously on another cell.

the `Completion.review` retains admitted evidence or its capture error.
`case_from_completion` and `Outbox::submit_completion` can export it after replay
without reevaluation or redispatch. A human resolution cannot release a terminal
idempotency key. Any later execution requires separately authorized input and a
new operation/key under normal policy.

Messages are bounded to 128 KiB, depth 64 and 50,000 JSON nodes. A phase retains
at most 4,096 changes and 64 proposals; a case has at most two ordered phases.
Identifiers contain at most 256 Unicode scalar characters. The outbox holds at
most 128 cases and 8 MiB serialized bytes. Evidence exceeding limits is rejected
whole rather than silently abbreviated. `contracts` applies equivalent semantic
checks on the TS consumer boundary.

Supplies the local queue, human
receipts and SLA alerts; consumes
those receipts as descriptive confidence priors. Hosts provide authentication,
durable storage, delivery, polling and notifications. This implementation has no
hosted queue, scheduler, automatic retry/execution or calibrated safety claim.

Contract: [§10.38](/agindow-runtime-contract.md#1038-ti-28--human-review-and-bounded-confidence-feedback).
Tests: `cargo test -p agw-guard --test review`.
