# Reviewed nested key repair

Use this local, unreleased source capability when an independently reviewed API
migration renames the same key inside known objects or selected array elements.
Prepare authoritative business facts off-path and start in observe mode. Existing
published package versions retain their documented top-level scope.

Add these fields to a reviewed Node `RenameRule` (illustrative rule excerpt):
```ts
{
  tool: 'contacts.create', side: 'request',
  from: 'old_name', to: 'display_name',
  parents: ['/contacts/0', '/contacts/2'],
  contract: 'contacts-v2', migration: 'name-migration-v1',
  provenance: 'owner-reviewed-change-123',
  expiresAt: 1798761600000, confidence: 1,
}
```
Python uses `source`, `target`, `parents=('/contacts/0', '/contacts/2')` and
`expires_at`; other field names match. Pass the rule through the existing
`selectiveRepair` / `SelectiveRepair` construction option with a local synchronous
verifier. It must verify intended meaning from independent application evidence;
schema validity or equality of copied values alone is insufficient.

For additional explicit old/new JSON-type constraints, attach the optional
[ v1 facts envelope](REPAIR_FACTS.md). Its scope, parent list, keys and
metadata must agree with the rule; neither facts nor a callback can override
preservation, confidence or policy gates. The reviewed nested metadata and
mandatory semantic verifier remain required when no envelope is attached.

In repair/strict mode, the shared Rust guest prepares one atomic set of key-token
edits. Each SDK independently checks exact preservation of all other captured
text, then invokes the semantic verifier. Expiry is checked before and after that
callback. Repair requires confidence >=0.97; strict requires >=0.99. Observe does
not apply. Repair events are local observations; simulation reports concrete
native escaped paths such as `contacts[0].old_name` and `contacts[2].old_name`.
No retry occurs, and upstream exceptions retain their identity.

A missing selected parent/member, destination collision or refusal leaves the
entire set unapplied. Repair retains shape passthrough; strict halts unresolved
matched drift. Duplicate keys, malformed/incomplete captures, exhausted work and
verifier faults use existing bypass with original identity. Statistical findings
and semantic blocks take precedence. Guest audit FIFO cannot be combined with
host-applied repair.

| Capability | Local scope / limit |
| --- | --- |
| Selection | 1–16 explicit RFC 6901 object-parent pointers per rule |
| Pointer syntax | Empty pointer selects root; `/` separates segments; `~0` is `~`, `~1` is `/` |
| Arrays | Canonical nonnegative decimal indices only; no wildcard expansion |
| Pointer bounds | 512 UTF-8 bytes, 32 segments per pointer |
| Literal keys | <=64 UTF-8 bytes; empty/punctuation keys supported; ASCII controls and unpaired surrogates refused |
| Rules | <=16 total, one per tool/side; duplicate/ancestor source selections fail setup |
| Facts | Reviewed contract/migration revisions, provenance, positive safe integer Unix-ms expiry, independent synchronous semantic verifier |
| Capture | Complete <=1 MiB, <=4096 nodes, depth <=64 |
| Work | Guest admission >=16 times capture bytes; existing three-way statistical/request/response budget split |
| Node | Normal callable boundary; existing raw string, Buffer and Uint8Array representations preserved |
| Python | Normal sync/async callable boundary over plain dict/list projections; no raw-body adapter |
| Preservation | Only selected member-name tokens change; all other raw text remains exact |
| Unsupported | Moves, container synthesis, automatic discovery, unit/enum rewrites, streaming and hosted qualification |

The records
source availability, not production qualification. The shared
[byte corpus](node/parity/nested-renames.json) runs through Rust, Node and Python;
SDK tests separately cover normal request/response dispatch and independent
preservation refusal.
