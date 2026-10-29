# Deterministic local source patches

`generate-source-patch` consumes the frozen evidence package and an exact
. It exports a local candidate patch and an
attributable receipt. It never writes the checkout, executes customer code,
resolves evidence references, calls a provider or opens a PR. Cortex remains the
off-path evidence producer; this CLI has no cross-cell transport dependency.
```sh
agindow generate-source-patch ./migration-request.json \
  --as-of 2026-10-20T12:00:01Z > ./candidate.json
```
The request contains these four objects:

| Object | Meaning |
|---|---|
| `checkout` | Current host-admitted checkout grant |
| `binding` | Exact manifest returned by `bind-source` |
| `evidence` | Validated `source_repair_evidence` v1 package |
| `grant` | Separate current generation authority, as below |

A generation grant names the exact scope/finding, evidence digest,
binding ID, reviewed fact, repository and actor:
```json
{
  "authority": {
    "tenant": "acme",
    "connection": "stripe-main",
    "operation": "write",
    "side": "request",
    "contract_revision": "cv_stripe_2026_10_17a",
    "evidence_revision": "evidence-1",
    "finding_id": "find_01HQ8Z4A5B6C7D8E9F0G1H2J3K",
    "finding_revision": 1
  },
  "evidenceDigest": "<exact evidence SHA-256>",
  "bindingId": "<exact binding SHA-256>",
  "factId": "migration-1",
  "repositoryId": "repo-a",
  "actorId": "owner",
  "authorityRevision": "generation-grant-1"
}
```
The CLI treats grants as explicit local maintainer assertions. A service host must
obtain current grants from authenticated owner policy rather than untrusted
request-body claims. Digests identify content and grant no permission. The host
owns repository mapping, authority refresh/revocation, checkout isolation and
artifact retention/disclosure. Generation admission does not grant publication.

The evaluation clock is explicit UTC and recorded in `receipt.evaluatedAt`;
service callers supply trusted current time. Historical local export can use an
older evidence clock and does not establish current freshness or authority for
publication. Revalidate authority, facts and base before any later action.

Recipe `js-ts-top-level-rename` version 1 replaces one exact AST token:
```ts
// Request: only the returned wire key changes; input.source remains local.
function serialize(input) { return { source: input.source }; }
// Candidate: return { payment_method: input.source };

// Response: only the bound wire member access changes.
function consume(result) { return result.source; }
// Candidate: return result.payment_method;
```
Single/double quotes, comments, imports, local type declarations, other symbols,
line endings and all bytes outside the token remain intact. Source syntax is
re-resolved after editing. This is syntax admission only: typechecking,
regression tests, real API acceptance and write correctness are unvalidated.
Now supplies a separate
;
its backend remains unconfigured by default. A patch may require additional
separately reviewed work before it passes a project's type/contract tests.

The current binding must match HEAD, committed blob bytes, actor authority and
all recorded fields. The selected fact must match operation/side/tenant and the
bound source field. Ambiguity, dynamic/computed keys, unsupported flow,
destination collisions, unsafe prototype fields, expired facts, altered digests,
stale bindings and dirty targets refuse. A fresh binding to a committed destination
field with the source field absent returns `noop/already_present`; dirty/stale old
bindings cannot establish a no-op. Inventory hints never select an edit.

Results are JSON: `generated` includes `patch`, `receipt`, `receiptDigest`; `noop`
includes a receipt and no patch; `refused` includes only a typed reason. Exit codes
are 0 for generated/no-op and 1 for refusal. The receipt records base SHA,
original/base-blob/result/patch/canonical-fact/evidence digests, binding/fact IDs,
actor/generation-authority revision and recipe version. `baseDigest` equals
`originalDigest` because dirty source is refused. No-op receipts hash an empty
patch. Receipt SHA-256 covers canonical receipt JSON. All receipts retain
`validation: unvalidated`; they are content identities, not signatures or proof.

Unchanged inputs and evaluation clock produce identical output. Unified diffs
contain three source-context lines around one changed region, Git-quoted UTF-8
paths and explicit missing-final-newline markers. The patch includes private
source context and remains a caller-owned local artifact. Request bytes are
capped at 64 KiB (evidence retains its 32 KiB limit); source/result at 1 MiB;
patch at 2 MiB + 16 KiB. All path, symlink and generated/vendor exclusions
remain active. There is no automatic application or publication option.
