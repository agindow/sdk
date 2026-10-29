# Local exact source bindings

This off-path CLI module separates authoritative maintainer selection from
`report`/`scanSources` inventory hints. It does not execute source, resolve imports,
install repositories, generate patches or publish anything. No hosted registry
route or persistence adapter is configured. The local manifest has format version
1; it is not a cross-cell message family.

Build the Node distribution, then run:
```sh
agindow bind-source ./binding-request.json > ./binding-result.json
agindow resolve-source ./resolution-request.json
```
Both commands print one JSON result and exit 0 for `bound`, 1 for `abstained`.
The caller chooses whether to persist stdout; the command never writes a checkout.
A binding request has this shape (replace the SHA and canonical checkout root):
```json
{
  "grant": {
    "root": "/absolute/canonical/checkout",
    "tenantId": "tenant-a",
    "repositoryId": "repo-a",
    "baseSha": "0123456789012345678901234567890123456789",
    "actorId": "maintainer-a",
    "authorityRevision": "owner-grant-1"
  },
  "selection": {
    "tenantId": "tenant-a",
    "repositoryId": "repo-a",
    "operation": "customer.create",
    "side": "request",
    "file": "packages/client/api.ts",
    "symbol": "serialize",
    "field": "source",
    "bindingVersion": 1
  }
}
```
A resolution request contains the **current** `grant` and the returned `manifest`
(instead of `selection`). Resolution re-reads committed/current bytes, rebuilds
AST spans and compares every recorded identity field. Source edits, stale HEAD,
changed authority and altered spans/digests abstain. A maintainer must explicitly
issue a new binding version when rebinding; this local module has no durable
version counter or revocation service.

The host must admit the tenant/repository/root/actor/authority revision from an
authenticated owner policy before calling `bindSource` or `resolveBinding` across
a trust boundary. The CLI treats the supplied grant as a local maintainer
assertion. A manifest's unkeyed SHA-256 identifies content and grants no access,
repair, generation or publication authority. Another tenant/repository selection
is rejected before source reads. This is not hosted membership validation.

Admitted syntax is a unique top-level function declaration or `const` function
with one identifier parameter and one direct returned expression:
```ts
export function serialize(input: { source: string }) {
  return { source: input.source };
}
export const consume = (result: { source: string }) => result.source;
```
Request selection identifies a returned object key; every property must be an
explicit key with a direct parameter-property value. Response selection identifies
a direct parameter-property access. Spans use UTF-16 offsets (end exclusive) and
one-based line/column; quoted object keys include their quotes. Names in other
objects/functions cannot select a target. Dynamic/computed keys, spreads,
shorthand, getters, aliases, nested functions, mutations, async/generator functions,
multiple matches and unsupported syntax abstain. Every refusal includes locations
and a manual selection/review path. Manual offsets cannot bypass the grammar.

Reads require an absolute canonical Git checkout root, exact HEAD and exact
committed blob bytes. Fixed read-only Git plumbing has a five-second timeout;
customer hooks, filters and scripts do not run. Request/source bytes are capped at
1 MiB, file paths at 64 components and 1024 characters; resolution reads one
explicit file and never walks the repository. Symlinks (including root ancestors),
traversal, absolute source paths, vendor/generated/build directories, declarations,
minified/generated filenames and generated markers are refused. Missing files,
Git failures and parse failures abstain. Known generated markers/path classes
cannot identify every arbitrary generated file; hosts must admit only maintained
source. The host owns checkout isolation and must prevent concurrent filesystem
mutation; these checks are not an OS sandbox against a hostile local process.
Existing bounded inventory behavior is unchanged.
