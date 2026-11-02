# Versioned SDK repair facts (, local unreleased source)

Use versioned facts when a reviewed rename must be restricted to a specific
contract revision and JSON value type. Load and review facts outside guarded
calls. The SDK does not discover facts or fetch them during execution.
Existing callers remain supported. No additional transformation recipe is
enabled by this admission boundary.

## Configure

Attach `facts` to a Node `RenameRule`. This is an illustrative rule for an
application whose owner reviewed this exact migration:
```ts
const rule = {
  tool: 'contacts.create', side: 'request',
  from: 'old_name', to: 'display_name', migration: 'contacts-v2', confidence: 1,
  facts: {
    version: 1, recipe: 'rename', tool: 'contacts.create', side: 'request',
    contract: 'contacts-schema-v2', migration: 'contacts-v2',
    provenance: 'owner-review-7', expiresAt: 4102444800000,
    parents: [''],
    old: { key: 'old_name', type: 'string' },
    new: { key: 'display_name', type: 'string' },
  },
} as const;
```
Pass the rule through the existing `selectiveRepair: { rules: [rule], verify }`
option. The required synchronous `verify({before, after, rule})` callback still
checks application meaning against independently prepared authority; returning
`true` cannot override a failed facts predicate or byte-preservation check.
Start in observe mode and evaluate before enabling repair.

Python exports frozen `RepairFacts` and `RepairConstraint` dataclasses. Construct
`RepairFacts` with the same fields, using `expires_at`, `parents=('',)`, and
`old=RepairConstraint('old_name', 'string')` /
`new=RepairConstraint('display_name', 'string')`. Attach it as `RenameRule.facts`
and supply the existing `SelectiveRepair(rules, verify)` option.

## Outcomes and limits

- Setup snapshots facts. Unsupported versions/recipes, missing required metadata,
Contradictory rule/fact revisions or scope, and overlapping rules raise
  `AgindowSetupError` before calls. A fact envelope cannot override a rule.
- `parents` must exactly match the rule's selected parents; root rules use `['']`
In Node or `('',)` in Python. Nested rule metadata remains required by.
- Constraints require source presence, destination absence, and the declared JSON
Value type before the edit, with inverse key presence after it. Supported types
Are `string`, `number`, `boolean`, `null`, `object`, `array`; rename requires the
Same old/new type. No coercion or value transformation occurs.
- The independent bounded token walk checks the entire original/patched text,
Including untouched context. Existing capture, node/depth and guest work
Limits remain; facts add no guest budget. Expiry uses Unix milliseconds and is
Checked again after the callback. Predicate mismatch or expiry is ordinary
Unresolved shape drift: repair passes through, strict halts.
- Faults, asynchronous callback answers and guest failures use the existing
Once-per-call availability bypass. Default/observe never apply repairs;
Statistical findings, semantic vetoes and confidence thresholds remain intact.
- Guest audit plus host-applied repair remains a setup error. Local repair events
Are metadata, not durable write proof. Hosted qualification remains pending.

See and
[contract §10.59](/agindow-runtime-contract.md#1059-planned-verified-repair-and-api-change-coverage-expansion).
