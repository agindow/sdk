# Reviewed scalar repair (local unreleased source)

Use this opt-in path when an independently reviewed migration declares a finite
string mapping, an empty-string null meaning, or an exact unit/epoch scale.
These additions implement / in source; they are not a claim about the
currently published npm/PyPI releases or hosted provider-write correctness.
The statistical guest never creates these facts or authorizes a recipe.

## Configure a reviewed migration

Build the local SDK first (`node cells/cell-relay/node/scripts/build.mjs`, then
`python3 cells/cell-relay/python/scripts/build.py --stage-only`). Add a scalar
rule to `selectiveRepair.rules` / `SelectiveRepair.rules`. Rename rules remain
supported. One rule per tool/side and the existing 16-rule maximum still apply.

Illustrative Node configuration (the application must supply its independent
synchronous `verifyMigration` predicate and load facts reviewed outside calls):
```ts
const facts = {
  version: 1, recipe: 'string-map', tool: 'payments.create', side: 'request',
  contract: 'payments-v2', migration: 'review-42', provenance: 'owner-review-42',
  expiresAt: 4102444800000, pointer: '/status',
  mapping: [['pending', 'awaiting_confirmation']],
} as const;
const sdk = createAgindow({ mode: 'repair', selectiveRepair: {
  rules: [{ recipe: 'scalar', tool: facts.tool, side: facts.side,
    migration: facts.migration, confidence: 1, facts }],
  verify: verifyMigration,
}});
await sdk.ready;
```
Python uses frozen `ScalarFacts` and `ScalarRule` dataclasses. Field names are
`expires_at`, `null_meaning`, `source_unit`, `target_unit`, `input_min`,
`input_max`, `output_min` and `output_max`; `mapping` is a tuple of string pairs.
Pass `SelectiveRepair((ScalarRule(tool, side, migration, confidence, facts),),
verify_migration)` to `Agindow(..., selective_repair=...)`. Sync and async
callables use the same guest and independent host verification.

## Expected behavior

The reviewed pointer identifies one existing scalar, including a concrete array
index. It is nonroot RFC 6901, at most 512 UTF-8 bytes and 32 segments. There is
no wildcard expansion, property invention, offset conversion or retry.

| Recipe | Required facts | Admission and refusal |
|---|---|---|
| `string-map` | `mapping`: 1–64 unique source/target pairs | Exact decoded strings, at most 256 UTF-8 bytes each. Unknown values have no fallback. Many-to-one targets require an `equivalence` list of `[source, reviewProvenance]` pairs for every source sharing a target. |
| `empty-null` | `nullMeaning` | Only the literal JSON token `""` becomes `null`. Missing properties, whitespace, numbers, null and nonempty strings refuse. |
| `scale` | Units, `numerator`, `denominator`, `inputMin`, `inputMax`, `outputMin`, `outputMax` | All ratios/bounds are exact numeric strings describing number-to-number conversion; payload numeric strings remain forbidden. Bounds apply before and after. |

Exact scale pairs are cents/dollars (1/100), seconds/milliseconds (1000/1),
and meters/millimeters (1000/1), in either direction. `numerator` and
`denominator` must match the selected pair. Currency exchange rates, identifiers,
timezone strings and arbitrary units have no supported recipe. Arithmetic
admits at most 36 coefficient digits, 18 fractional places and exponent magnitude
18. Overflow, excessive precision, nonterminating decimals, offsets and missing
facts refuse rather than round. Zero can be a no-op.

Preparation uses the shared Rust raw-span repair engine with the semantic.99
threshold. The SDK independently reproduces the exact token edit and arithmetic,
checks untouched bytes, and calls the application verifier. Expiry is checked
again after verification. Ordinary object projections additionally refuse any
changed numeric token that cannot round-trip through the host's JSON numbers.
Node raw string/Buffer/Uint8Array inputs preserve representation and exact
numeric tokens. Python supports its existing plain dict/list projections;
raw lexical parity is tested directly at the shared guest boundary.

Observe mode never applies or blocks. Refused semantic writes remain blocked in
repair/strict; reads remain open. Verifier faults use the existing availability
bypass and dispatch at most once. Existing statistical vetoes are never cleared.
`repairRefusal` / `repair_refusal` on the local verdict distinguishes source,
confidence/policy, stale-facts, verifier-refused and projection-precision refusal.
No default changes and no synchronous network, database or other-cell access.
Guest audit remains incompatible with selective repair at setup.

See [contract §10.59](/agindow-runtime-contract.md#1059-planned-verified-repair-and-api-change-coverage-expansion),
[repair facts](REPAIR_FACTS.md), and [nested renames](NESTED_REPAIR.md).
