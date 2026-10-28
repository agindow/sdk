# Reviewed keyless registration renewal

Implements approach B as an opt-in private control plane. Local compiled
fixtures exist; live operation is unqualified under
. These configs do not
deploy or enable anything by themselves. Manual bring-up remains supported.

**Operator cutover (private, not public docs):**
and [operator/smoke-token.mjs](operator/smoke-token.mjs) for the first `/v1/token`.

## Components and configuration

| Component | Configuration | Required bindings/material |
|---|---|---|
| Execution host, auto mode | `../auto.wrangler.jsonc` | Same AUTHORITY namespace/class and origin; GUARD_CONNECTIONS; KEYLESS_REVIEW_KEYS; KEYLESS_DELIVERY_TOKEN |
| Private policy authority | `wrangler.jsonc` | POLICIES SQLite DO; EXECUTORS pointing to the existing guard namespace; origin/profile; reviewer public trust; delivery capability |
| Renewal job | `job.wrangler.jsonc` | Only a RENEWAL service binding to the control Worker's named Renewal entrypoint; cron and enable flag |
| Operator transport | [`operator/`](operator/README.md) or host-owned binding | Bearer-gated HTTP to Administration, or a private service binding; offline signed commands |

The execution host uses `KEYLESS_POLICY_MODE=auto`, `KEYLESS_PROFILE=primary` and
an exact `EXECUTION_ORIGIN`. Its `KEYLESS_ENABLED` switch still gates execution.
The control Worker uses `KEYLESS_CONTROL_ENABLED`; the job separately uses
`KEYLESS_RENEWAL_ENABLED`. All checked-in enable flags are false. The job holds
no reviewer private key, Cloudflare secret/deploy credential or execution binding.
The default HTTP handlers of both control and job Workers return 404. Do not add
an unauthenticated HTTP wrapper around the administrative entrypoint. Supplies
an opt-in bearer-gated operator Worker; hosts may still use their own private binding.

`KEYLESS_REVIEW_KEYS` is private JSON containing 1–2 exact objects:
`[{"kid":"reviewer-1","x":"<Ed25519 public JWK coordinate>","until":<unix seconds>}]`.
`until` must cover the mandate, and the selected key must be current. Keep the
reviewer private key outside every runtime. It is separate from workload assertion
keys and the DO's transient token-signing key. The host receives no workload
private key. There are no legacy GUARD_KEYS in auto mode.

`KEYLESS_DELIVERY_TOKEN` is an independently generated 43–128 character base64url
host capability, installed privately only in authority and executor Workers.
Never install it in the renewal job or a workload. Both transport and review
signature checks constrain snapshot installation. Capability rotation requires a
coordinated stopped update of both hosts, followed by delivery verification; there
is no implicit fallback to an old capability. This capability is privileged
authority/executor material: the residual revocation bound assumes it remains
intact. Its compromise requires execution shutdown and coordinated replacement.
The scheduler capability alone does not confer these delivery privileges.

`KEYLESS_RENEWAL_SECONDS` on the authority defaults to 600 and accepts 60–600.
If changed, adjust the job cron to run at least that often. Repeated early ticks
return the existing lease; they do not extend it. The authority delivers batches
of at most 16 owners and persists remaining targets. Failed delivery schedules a
60s per-owner retry; healthy backlog continues after one second, and previously
undelivered owners retain priority across renewal. Alarms deliver existing leases only. Scheduler failure therefore cannot
be hidden by an alarm that independently renews leases.

## Initial admission and cutover

1. Stop execution deliberately and verify propagation. Preserve the guard Worker,
   AUTHORITY namespace/class, assertion ledger and owner ID prefix. Provision
Control/job bindings while their enable flags remain false. Do not copy a
Namespace or let two policy authorities own one profile.
2. Install reviewer public trust, the delivery capability and fixed connection
Bindings through private host operations. Select auto mode exclusively; it
Requires no KEYLESS_REGISTRATIONS secret and never reads an old one as fallback.
   Owners that have adopted auto state reject a manual-mode rollback.
3. Enable the private control Worker. Through Administration
Binding, call `inspect`. Its JSON includes the storage generation and current
Revision. Protect that output: it includes reviewed public grants and signed
Review evidence, even though it contains no workload private key.
4. Prepare a request file with **exactly** the fields below (real rows are private
Operator data; this is a synthetic format example). Choose a new random command
   ID, current issue time, expiry at most 300s later and review deadline at most
   30 days after issue time, covered by the reviewer key deadline.
```json
{
  "version": 1,
  "origin": "https://guard.agindow.com",
  "profile": "primary",
  "generation": "<generation from inspect>",
  "previousGeneration": null,
  "expectedRevision": 0,
  "id": "<new random command ID>",
  "kid": "reviewer-1",
  "iat": 1800000000,
  "exp": 1800000120,
  "action": "admit",
  "reviewUntil": 1800086400,
  "rows": [{
    "id": "workload", "kid": "client-key", "x": "<workload public coordinate>",
    "audience": "https://guard.agindow.com/v1/token", "tenant": "<tenant>",
    "environment": "test", "connection": "<fixed handle>", "tools": ["<safe read>"]
  }]
}
```
The example timestamps are placeholders; the offline signer rejects expired or
future requests. From the repository root, with dependencies installed:
```sh
node cells/cell-relay/hosted/owner/control/sign-review.mjs request.json reviewer.pem command.json
```
This command signs offline, calculates the canonical digest and writes a new
0600 file exclusively. It never submits, deploys, prints key bytes or overwrites
an existing artifact. Only public workload coordinates belong in rows.

5. Submit the resulting text through `Administration.submit(commandText)` on your
Private operator transport. Repeating the exact command before expiry returns
The retained receipt. A conflicting expected revision requires reading current
State and obtaining a newly signed review, not automatic retry against a newer
Revision. Signature/shape failures consume no command receipt.
6. Wait for bounded background delivery and require the intended owner's effective
Acknowledgement. `Administration.inspect` includes pending target IDs;
Renewal `health` reports desired revision/sequence and pending count. Pending
Does not mean protected/current. Enable the job after cutover, then explicitly
Enable execution. Fresh replay owners still wait 360s; renewal does not shorten it.

## Review, expiry and rotation

The canonical SHA-256 digest commits exact origin/profile and every registration
field; rows/tools sort by Unicode code point, with no string normalization.
Duplicate IDs/tools and unknown fields reject. The complete registration set has
both the existing row/count limits and a 262144 UTF-8-byte bound.

Only `admit` replaces scope/key content. `resume` requires the same digest and
cannot revive a revoked review; revoked grants require a new `admit`. `stop` and
`revoke` use the current reviewed rows/digest and a fresh signed command, advancing
the review revision. A late automatic renewal latches expired authority; it
requires signed resume/admission, even if a prior review mandate has time left.
A current authority may redeliver an already-issued unexpired newer lease after
an owner's delivery lag. This is not a new extension at the owner.

Each lease is at most 3600s and never exceeds the mandate. A changed/corrupt digest
halts renewal with a closed `review_drift` failure; it is never copied into a new
baseline. Observe `lastFailure`, expiry, revision/sequence and pending delivery.
Monitor pending owners' last acknowledged state through your host's receipt
collection; a central renewed lease is not proof of delivery. Configure operator
alerts for failures and less than 1200s effective-lease headroom. Log forwarding,
alert destination and retention remain explicit host integration; this package
emits closed structured failures and private health rather than inventing delivery.

For reviewer rotation, install at most two public keys with explicit finite
expiries, sign a higher-revision admission using the new reviewer, confirm delivery,
then remove the old public key. Old signed commands then fail verification.
Public workload-key rotation also requires reviewed admission; it is never renewal.

Valid already-issued tokens continue under local token TTL/epoch/scope rules when
renewal fails. New exchange/delegation requires a current admission lease. Guards
and retained GETs do no renewal SQL/RPC/IdP work, and never initiate renewal.
Private snapshot updates/initialization own the SQL. A missing/cold validation
context fails unavailable rather than fetching it synchronously.

## Stop and recovery

- Commit signed stop/revoke, then disable the job. Delivery alarms cannot renew.
  Stopping only the job leaves the last lease valid until its absolute expiry;
It is not instantaneous credential revocation. Disabling only the control Worker
Similarly leaves already-delivered state bounded by that expiry.
- With <=30s pairwise clock skew and stable relative offsets over a lease/token
Lifetime, stale owners can retain issuance authority for 3630s,
Then the final token's <=330s lifetime/skew: conservative **3960s / 66 minutes**.
  Healthy delivery shortens this; no instantaneous distributed revocation is claimed.
  Those clock assumptions require live verification; unbounded relative clock drift
Has no qualified finite wall-clock bound.
- For emergency execution shutdown, set `KEYLESS_ENABLED=false` at ingress and
Execution owners and verify actual deployment propagation. Stop/revoke review
And remove compromised trust as appropriate. Re-enabling execution does not
Clear a stopped/revoked policy. Restart still rotates the ephemeral issuer epoch.
- Empty/lost policy storage creates a fresh random generation and issues nothing.
  Re-inspect and sign a fresh `admit`, explicitly naming the old owner-policy
Generation in `previousGeneration`. Existing owners refuse an unapproved handoff.
  Lost assertion storage independently retains 's full 360s quarantine.
- Stop before restore/PITR or namespace migration. Establish a fresh generation
And new review; arbitrary non-empty rollback cannot be detected solely from
Restored storage. Do not restore revoked policy and call it transparent recovery.

## Local verification
```sh
pnpm --filter cell-relay check:owner
pnpm --filter cell-relay test:renewal
pnpm --filter cell-relay test:renewal:mutations
pnpm --filter cell-relay test:hosted
pnpm --filter cell-relay build:renewal
pnpm check
```
`build:renewal` packages the disabled control and job Workers using dry runs.
`build:auto-owner` packages the disabled auto-mode guard after the existing native
host build. None deploys. Generated types use Wrangler with all referenced Worker
configs; do not hand-edit declarations. Live routes, operator transport, clocks,
Cloudflare storage/failover and HTTPS qualification remain work.

## Customer registration

The separate accepts only
verified owner intents through identity. It holds a constrained platform reviewer
key; this authority independently enforces its installed public policy. Renewal
remains unchanged-row-only. Remains operator transport and is not mounted
as a customer registration API. Host policy/key custody and enablement are explicit.
