# Private operator cutover — keyless auto mode

**Operator-private.** Do not publish this file through `contracts/docs`, the
public Worker, sitemap, or marketing pages. It describes host cutover only.
Live qualification remains.

Related: [control/README.md](control/README.md), [control/operator/README.md](control/operator/README.md),
,.

## Goal

Move `agindow-keyless-guard` from manual `KEYLESS_REGISTRATIONS` hourly secrets
to auto mode: reviewed policy authority + renewal job + bearer-gated operator
transport. Preserve Worker name, `AUTHORITY` / `KeylessExecutionOwner`, and
`https://guard.agindow.com`.

## Inventory

| Worker | Config | Public route |
|---|---|---|
| `agindow-keyless-guard` | `auto.wrangler.jsonc` | `guard.agindow.com` |
| `agindow-keyless-policy` | `control/wrangler.jsonc` | none |
| `agindow-keyless-renewal` | `control/job.wrangler.jsonc` | none (cron) |
| `agindow-keyless-operator` | `control/operator/wrangler.jsonc` | `*.workers.dev` bring-up only |

## Material (offline)

- Workload Ed25519 PEM (client assertion) — never install on Workers
- Reviewer Ed25519 PEM (signs admit/resume/stop/revoke) — never install on Workers
- `KEYLESS_REVIEW_KEYS` — public reviewer `kid`/`x`/`until` on guard + policy
- `KEYLESS_DELIVERY_TOKEN` — same value on guard + policy; not on job or operator
- `GUARD_CONNECTIONS` — fixed `connection/tool` upstreams
- `OPERATOR_TOKEN` — operator Worker only

## Ordered cutover

1. Keep `KEYLESS_ENABLED=false` until delivery ack.
2. Deploy policy (secrets file on first create) then renewal job (`KEYLESS_RENEWAL_ENABLED=false`).
3. Redeploy guard with `auto.wrangler.jsonc` (no `KEYLESS_REGISTRATIONS`).
4. Deploy operator Worker; redeploy policy with `KEYLESS_CONTROL_ENABLED=true`.
5. `GET /inspect` via operator → note `generation`, `revision`.
6. Build admit request → `node …/sign-review.mjs request.json reviewer.pem command.json`.
7. `POST /submit` command before its `exp` (≤300s after `iat`).
8. Re-`inspect` until `pending` is empty (owner ack).
9. Set `KEYLESS_RENEWAL_ENABLED=true`, redeploy job.
10. Set `KEYLESS_ENABLED=true`, redeploy auto guard.
11. Wait up to **360s** after empty assertion ledger before first successful `/v1/token`.
12. Smoke `/v1/token` then `/v1/guard` (read). See below.

Owners that adopted auto refuse rollback to manual `KEYLESS_REGISTRATIONS`.

## First `/v1/token` smoke

Endpoint: `https://guard.agindow.com/v1/token`
Audience for client assertion: `https://guard.agindow.com/v1/token`
Access-token audience: `agindow:guard`
Registration / iss / sub: admitted `id` (e.g. `workload`)
Kid: admitted workload `kid`
Tools: exact admitted tools (e.g. `["op"]`)

Body shape:
```json
{
  "grant_type": "urn:ietf:params:oauth:grant-type:token-exchange",
  "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
  "subject_token": "<Ed25519 JWT assertion>",
  "registration": "workload",
  "audience": "agindow:guard",
  "tools": ["op"],
  "ttl": 300,
  "cnf": { "jkt": "<DPoP public JWK thumbprint>" }
}
```
Helpers (redact tokens; private operator scripts only):
```sh
node cells/cell-relay/hosted/owner/control/operator/smoke-token.mjs \
  "$HOME/Desktop/agindow-workload-private.pem"

node cells/cell-relay/hosted/owner/control/operator/smoke-guard.mjs \
  "$HOME/Desktop/agindow-workload-private.pem"
```
`smoke-token` expects `200` and `token_type=DPoP` after quarantine.
`smoke-guard` exchanges a token, binds a DPoP proof to `POST /v1/guard`, and calls
`connection=c` / `tool=op` / `operation=read` / `mode=observe` (upstream from
`GUARD_CONNECTIONS`). Expect `200` and an upstream body prefix when the read
binding is reachable.

`503 identity_warming` shortly after enable is empty-owner warm-up; wait and
retry. `401` is assertion/DPoP/scope failure. Browser GET `/` returning
`not_found` after enable is normal.

## Day-2

- Renewal job extends **unchanged** rows about every 10 minutes; lease ≤1h; mandate ≤30d.
- Scope/key change → new signed `admit`, not renewal.
- Emergency execution off → `KEYLESS_ENABLED=false` + verify deploy.
- Hardening: move operator off open `workers.dev` (private domain / Zero Trust).
- Delete `/tmp/agindow-*` secrets files after copying PEMs somewhere durable.

## What this does not claim

Detection/repair calibration, partition/failover, customer self-service
registration, or that `workers.dev` admin is production-hardened.

## pre-alpha smoke retirement

There are no existing customers (user clarification, October 28, 2026). The old
`primary` profile / tenant `a` at `guard.agindow.com` is a pre-alpha smoke fixture.
No customer migration or backwards-compatibility rollout is required. Retire the
fixture explicitly; there is no import into the multi-org directory or baseline.

When a host switch is separately authorized:

1. Disable old ingress/owner execution, verify propagation, and stop/revoke the
Old reviewed smoke policy with its offline recovery authority. Disabling new
Admission alone does not stop issued credentials. Observe 's conservative
   66-minute residual validity bound and host clock assumptions before reuse.
2. Keep old storage/receipts as evidence. Use the distinct disabled multi-org
   Worker names/namespaces; do not rename owner IDs, copy `primary` rows, or share
Its admission/policy ledger. The v2 owner rejects legacy registration IDs.
3. Follow the [host-wide setup](admission/README.md). There is no org-specific
   `ADMISSION_POLICY.tenant` or `bootstrapGeneration` setup. Test first registration
From two eligible orgs, acknowledged delivery, 360-second quarantine and actual
Read dispatch with new public handoffs before switching the public ingress.
4. Switch ingress only after these checks and explicit deployment authorization.
   The identity flag remains false until all host bindings/trust/catalog exist.
   Rollback means disabling v2 execution and restoring a separately reviewed smoke
Setup if needed; it never silently maps new customer IDs onto old storage.

This story performs no production switch and does not complete. These
steps retire test infrastructure; they do not claim customer migration evidence.
