# Local machine identity — EPIC-14

Contract the runtime contract and
the wrapper API define the
per-surface decision. This is opt-in local code. Nothing is deployed, no live
identity provider is qualified, and the SDK's embedded behavior is unchanged.

The Rust API lives in `agw_http_host::machine`. `KeyCache::validate` checks
signature and route authority; `MachineGate::admit` also requires possession.
Never use successful JWT parsing as authorization. Route context is trusted host
configuration. All key loading, registration and identity acquisition happen
outside guard decisions; the validator has no HTTP/database/discovery client.

Access JWTs, private_key_jwt and DPoP use Ed25519/EdDSA. External OIDC also
accepts ES256; SPIFFE JWT-SVID uses ES256 with standard sub/aud/exp claims.
RS256 provider adapters, X.509 SVID
exchange and HTTP Message Signatures are not implemented. This does not imply
live compatibility with GitHub, AWS, GCP or Azure. Public-key client assertions
provide the self-hosted path selected by. Their issuer and subject must
both equal the registered client ID; only the current registered kid can mint.

## SDK setup

Node creates an ephemeral Ed25519 proof key in process:

```ts
import { createRemoteAuth, workloadIdentity, guardRemote } from 'agindow';

const auth = await createRemoteAuth({
  acquire: workloadIdentity({
    exchangeEndpoint: 'https://guard.example/v1/token',
    registration: 'registered-workload', tools: ['read'],
    // Or supply an assertion callback for a platform endpoint/workload API.
    // AGINDOW_IDENTITY_TOKEN_FILE is read automatically when tokenFile is absent.
  }),
  onDiagnostic: reason => console.info(reason), // fixed labels only
});
try {
  await guardRemote(original, envelope, { endpoint: 'https://guard.example/v1/guard', auth });
} finally { auth.close; }
```

Python uses `RemoteAuth(workload_identity(...), ProofSigner(public_x, sign))`
and `RemoteOptions(endpoint, auth=auth)`. `public_x` is the base64url Ed25519
public coordinate; `sign(bytes)` returns a 64-byte signature through the host's
cryptographic library/HSM. The SDK adds no private-key serialization or storage.
Python performs initial acquisition synchronously at setup; `guard_remote_async`
keeps blocking transport off the event loop. Close sessions to stop their daemon
refresh timer. Assertions may come from a bounded file or host callback; the
SDK does not implement cloud-specific metadata discovery or SPIFFE socket RPCs.

Tokens remain in memory. Refresh starts 60s before expiry, retries after 5s on
failure, and retains an unexpired token. No call starts an exchange; expired or
unavailable auth invokes the original binding with one bypass. A 401/403 stays
an auth rejection and never invokes fallback. Recovery creates a new proof bound
to its GET URL. Credentials/headers must remain inside the SDK process boundary.
Shared fixtures in `node/parity/auth*.json` verify lifecycle boundaries and exact
DPoP bytes across Rust, Node crypto and Python's injected signer.

Legacy auth is explicit: a direct `apiKey`/`api_key` is accepted only in the
`agw_live_` or `agw_test_` form with 32–256 base64url characters. Its diagnostic
callback receives `legacy_unbound`. In a keyless session, fallback requires both
`allowLegacyFallback`/`allow_legacy_fallback` and a valid legacy key. Combining
an auth session and direct API key is a setup error. The session never silently
substitutes the key after refresh failure. still owns issuance/revocation.

## Hosted configuration

The existing Worker explicitly enables `GUARD_MACHINE_AUTH`. This is an
administrator-supplied JSON snapshot with:

- `issuer`: the exact Agindow signing issuer.
- `keys`: 1–64 `{ issuer, kid, x, until }` public Ed25519 keys. `until` is Unix
  seconds, at most 300s after refresh. Old and new kids may overlap.
- `keyless_only`: tenant IDs that reject every legacy key.
- `legacy_until`: absolute overlap deadline, at most 900s after configuration.
  Zero disables legacy auth for every tenant under this snapshot.

Connection bindings retain their fixed tenant/URL/operation, with an optional
`environment` (default `live`). Request data cannot override that context.
Keyless-only operation does not need `GUARD_KEYS`. Token quotas are bounded,
isolate-local 1000 calls per tenant per minute, not billing entitlements.
DPoP uses the `DPoP` authorization scheme. mTLS uses `Bearer` with a certificate
bound token; only Cloudflare-attested `cf.tlsClientAuth`, verified `SUCCESS`,
supplies its SHA-256 peer fingerprint. Client headers never establish TLS identity.
Local tests qualify proof enforcement; real client-certificate termination is
not provider-qualified. Bound proof checks cover audit/outcome retrieval too.
Those retained outcomes still require the same token and are process-local.

`POST /v1/token` enables the control-plane exchange with a separate administrator
secret `GUARD_IDENTITY_EXCHANGE`:

- `issuer`, `kid`, `signing_seed`: the Agindow issuer, current signing kid and
  base64url 32-byte Agindow signing seed. This is Agindow's key, never a vendor key.
- `keys`: up to 64 provider `{ issuer, kid, x, until }` snapshots; add `y`
  for a P-256 key (both coordinates are 32-byte base64url). Include the
  Agindow public key here too if delegation is enabled.
- `registrations`: up to 256 `{ id, kid, audiences, issuer, subject, audience,
  tenant, environment, connection, tools, current, private_key_jwt }` grants.
  `audience` is the assertion audience; `audiences` is the allowed output set.
  Subjects are exact matches, never patterns.

The registration source must be populated only by authenticated tenant admins.
This implementation exposes no public registration endpoint. Host push/refresh,
durable registration storage and issuer discovery are explicit obligations;
unchanged stale snapshots return 503 instead of fetching keys synchronously.
Public snapshot replacement retains live replay/rate fences. Signing instances
use a fresh host random ID so minted token IDs do not repeat after restart.

The local exchange accepts a bounded JSON RFC 8693 profile:
`grant_type=urn:ietf:params:oauth:grant-type:token-exchange`,
`subject_token_type=urn:ietf:params:oauth:token-type:jwt`, `subject_token`,
`registration`, `audience`, `tools`, `ttl` and `cnf`. It returns `access_token`,
`token_type`, `issued_token_type`, `expires_in`, `expires_at` with no-store headers.
It is a JSON profile, not a claim of complete OAuth discovery/form interoperability.
Tokens/assertions never appear in its error responses.

Delegation uses the access-token subject type, parent token, DPoP header,
connection, tools, TTL, child subject and delegatee `cnf`. The fixed route binding
supplies tenant/environment. Possession of the parent is mandatory. Child scopes
only narrow, expiry never exceeds the parent, depth defaults to three and every
actor's scope/expiry is checked. The private `agw_act` claim carries the
bounded authority chain; this does not redefine RFC 8693's `act` object. Audit metadata records actor IDs/scopes/expiries,
never token/proof values. Removing a registration stops new exchange immediately;
issued tokens/descendants remain valid for at most their original TTL plus 30s.
No individual-token revocation service is claimed.

## Verification and limits

Native suites cover claim defects, expiry/skew edges, key staleness/rotation,
registration changes, cross-tenant confusion, exact subject/scope matching,
private assertion replay/revocation, delegation depth/intersection, mTLS mismatch,
stolen DPoP tokens, bounded replay saturation, keyless-only mode and audit output.
The compiled Worker suite also exercises guard authorization and byte identity.
Node and Python lifecycle fixtures cover refresh failure and single bypass.

Replay/rate state is process-local. Live replay fences are never evicted to admit
a new proof; full state rejects with counters. Restart/isolate eviction loses
those fences. Production routing must keep a token's proof traffic at one replay
authority or provide distributed enforcement before claiming global replay
resistance. No live deployment or production latency qualification is implied.
Distributed replay enforcement is tracked in.

Standards: [DPoP RFC 9449](https://www.rfc-editor.org/rfc/rfc9449.html),
[token exchange RFC 8693](https://www.rfc-editor.org/rfc/rfc8693.html),
[client assertions RFC 7523](https://www.rfc-editor.org/rfc/rfc7523.html).

Local native fixture measurement on 2026-10-22: access-JWT plus DPoP validation
mean 82.42 µs, p99 100.96 µs over 1000 calls, release profile. This is additional
auth cost, reported separately from the frozen engine budgets, not a claim that
crypto verification fits the <10 µs engine bypass budget. Reproduce with
`cargo run -p agw-http-host --release --example machine_auth_latency`.
