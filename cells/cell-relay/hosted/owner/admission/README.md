# Customer workload admission — / (extends / )

This is an off-path private service, separate from the execution and renewal
Workers. Its default HTTP handler is 404. Bind only the verified-session identity
Worker to the `CustomerWorkloads` entrypoint; do not expose it through 's
operator bearer or a public route. Local fixtures are not qualification.

## Operator setup — once per host

No per-organization tenant put, generation secret or CLI admit is required. The
host policy is tenant-free. The private identity path supplies the verified owner
membership; the browser cannot choose the registration tenant. Defaults remain
disabled and no hosted qualification is implied.

1. Prepare the separate disabled v2 configs: [admission](multi.wrangler.jsonc),
  ,
And. They name separate
Namespaces from the old `primary` smoke host and publish no public admin HTTP.
   Bind admission's `ADMINISTRATION` to `MultiAdministration`; bind the job's
   `RENEWAL` to `MultiRenewal`. The renewal job gets neither signer nor admission
Capability. Review
Before an explicitly authorized ingress switch.
2. Approve one closed host policy, shown here as a shape (replace the deadline
And host-specific connection/tool before use):
```js
   {format: 2, version: "host-v1", reviewerKid: "platform-v1",
    origin: "https://guard.agindow.com", profileKey: "customer",
    environment: "test", connection: "catalog-read", tools: ["fetch"],
    audience: "https://guard.agindow.com/v1/token", until: 0}
   ```
   `until` must be a future absolute Unix deadline, bounded by reviewer trust.
   There is no `tenant`, `bootstrapGeneration`, wildcard, live or write permission.
   Set the identical policy as `ADMISSION_POLICY` here and `KEYLESS_HOST_POLICY`
On authority and owners. Profile IDs are `org2-<sha256>` of the exact JSON tuple
   `["agindow-org-profile-v2", origin, profileKey, verifiedOrgId, "test"]`.
3. Generate a separate platform Ed25519 reviewer in private host custody. Put
   `ADMISSION_SIGNING_JWK` **only** on admission and set `ADMISSION_REVIEW_KID`.
   Install its constrained public `{kid,x,until,policy}` reviewer entry on
Authority and owners. `policy` is the host policy above, with no tenant. Keep
The offline recovery reviewer separate; at most two reviewer entries are
Supported. A policy change requires a new version and reviewer kid. Never add
An unrestricted duplicate of the platform key. Neither identity, job, owners,
Browser nor transport receives this signing private key.
4. Set the same host-approved `KEYLESS_READ_CATALOG` on admission and owners:
   `{"catalog-read/fetch":{"url":"https://vendor.example/read","method":"GET","operation":"read"}}`.
   Entries are exact routes with no tenant/environment grant, wildcard, credential
Or write operation. Admission validates this catalog before opening a profile.
   Owners derive concrete tenant/Test/read `GUARD_CONNECTIONS` locally from the
Verified registration row and catalog. Missing or invalid entries refuse
Readiness/execution. There is no dashboard/database/directory call on guard.
5. Install a separate >=43-character base64url readiness capability as
   `ADMISSION_READINESS_TOKEN` here and `KEYLESS_READINESS_TOKEN` on owners. It
Permits a private readiness read, not installation. Keep the separate delivery
Token confined to authority/owners. Disable body and credential logging at
Ingress; workload private keys never enter AGINDOW Workers or dashboard storage.
6. Merge the
Into the production environment of the existing identity config, preserving
Its other vars/services. The default identity config has no customer binding.
   Bind `CUSTOMER_WORKLOADS` to this Worker's `CustomerWorkloads` entrypoint; set
   `WORKLOAD_SELF_SERVE_ENABLED=true` only after host setup. Enable admission's
   `FEATURE_FLAGS=workload-self-serve`, authority control, owner execution and the
Renewal job independently only after policy/trust/catalog agree. Preview stays
Unconfigured. This implementation does not deploy or enable any host.
7. Exercise two verified organizations without changing host policy: create,
Delivery, six-minute quarantine, fresh readiness and real signed SDK reads.
   Verify cross-org rejection and disabled/missing-backend UI. Record deployment,
Clock/routing/monitoring evidence separately under before hosted claims.

## First registration and renewal

An eligible owner create validates public input, session namespace, connection
and catalog before provisioning. The private host directory reserves a random
nonce, opens the deterministic per-org authority, and pins its fresh generation.
The authority persists tenant/profile/generation/nonce before admission; the
admission ledger separately pins its tenant and policy history. A directory loss
cannot adopt an old authority with a different nonce. An authority generation
change cannot match the stored pin. A fresh admission ledger cannot import an
active baseline. These are refusal boundaries, not arbitrary rollback recovery.

List alone returns an empty eligible catalog without enrolling a directory profile
or granting execution. The directory holds at most 1024 profiles, including pending
reservations. It stores locators and generation pins, never a mixed-tenant baseline.
Each org has separate admission/policy objects and each registration routes to its
own immutable execution-owner namespace. Mixed-tenant rows are rejected even with
a valid offline signature. Legacy IDs cannot install on the v2 owners.

`MultiRenewal` accepts no tenant or row input. It visits only enrolled profiles,
processes at most 16 per batch, and rearms alarms until its cursor reaches the end.
A failed profile cannot starve later ones. It cannot enroll, sign or admit, and
each authority still verifies the unchanged review and original mandate deadline.

The format-1 configs remain explicit legacy test/recovery fixtures only. Their
single-tenant bootstrap is not the customer path and is not imported into v2.

## Runtime operation and limits

Customer create/rotate/revoke intents are public-only and attributed, stored in
SQLite before signing. The authority independently constrains customer reviewer
commands. One changed row per command, no stopped-profile resume, no cross-tenant
baseline and no extension of an existing mandate are allowed. A signed command
uses generation/revision CAS and a 120-second lifetime. Up to three proven CAS
rebases are attempted; uncertain commits block later mutation until reconciled.
Unsigned intents expire after five minutes; command expiry cannot extend that
deadline. Committed receipts retain the signed canonical command tuple and
signature, not repeated full-profile payloads. The persisted alarm retries
submitted work in batches of 16, explicitly rearming at 60 seconds while eligible
work remains. Disabling the feature stops further alarm retries. It never renews a lease. If the signing service
is down, delivery and unchanged-row renewal remain independent.

Policy history is capped at 32 immutable versions; changing a version in place
or reusing the store for another tenant/profile/generation fails closed. A new
admission store requires an empty authority; importing an active baseline is
not automatic. The store admits at most 1024 intent receipts and 256 active registration rows;
receipts and revoked workload tombstones are retained without automatic eviction.
At capacity it refuses further work. Plan explicit off-path archival/profile
replacement under operator control; resetting storage must never erase live
idempotency/revocation fences. Rolling-hour limits are 10 intents per actor and
50 per tenant. Invalid/private input is rejected before an intent is stored.
Readiness expires after at most 30 seconds and requires a current acknowledged
owner revision, read routes, quarantine completion and successful native issuer
preparation using a deliberately invalid assertion (no token or vendor dispatch).
Only the customer's signed exchange proves possession of its key.

Rotating the customer public key has no automatic overlap. Revoking the last
row commits explicit revoked status; new create cannot resume it. Expired/stopped
profiles require offline operator recovery. New self-service requests cannot
extend another registration's mandate. Existing-token validation remains local;
renewal, dashboard and SQL are not new guard dependencies.

For emergency stop, disable ingress/owner execution, verify propagation and
revoke/stop the reviewed policy with the offline operator signer. Disabling only
this service stops new intents, not issued credentials. 's conservative
66-minute residual lease/token bound and clock assumptions remain applicable.
Rotate reviewer trust by disabling customer admission, installing a fresh bounded
policy/key, reviewing the current baseline and verifying all owner acknowledgements
before re-enabling. Lost admission storage or arbitrary rollback requires stopped
host recovery and a fresh authority generation, not automatic import or resurrection.

## Customer documentation

The public guide is owned by `contracts/src/docs/keyless-guide.ts` and exposed
at `/docs/guides/keyless-auth`. It contains no operator tokens, private reviewer
material or private runbook links. Public handoff fields can be viewed again;
there is no one-time secret or private-key reveal in this flow.

## Local validation

Build the relay SDK and hosted native WASM first. Hosted and cross-cell session
suites are explicit gates outside the main repository test command:
```sh
pnpm --filter cell-relay test:hosted
node cells/cell-relay/hosted/mutate-admission.mjs
node cells/cell-relay/hosted/mutate-multi.mjs
pnpm --filter cell-registry exec vitest run --config vitest.workloads.config.ts
```
The local fixtures exercise two real verified dashboard sessions under one host
policy, both published SDK helpers and bounded renewal across 17 organizations.
They do not establish a public host, external provider or partition qualification.
