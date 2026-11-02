# Operation-scoped JSON comparison (local unreleased source)

Use this explicit advisory comparison to inspect supplied declarations or two
complete JSON observations for one operation. It builds on the native shape
diff. It does not feed new findings into guard policy, authorize repair, fetch
OpenAPI documents or maintain a shared baseline cache.

## Compare one operation

Node exports async `compareOperation({baseline, current}, work?)`. Python exports
`compare_operation(baseline, current, work=...)` with frozen `OperationScope` and
`OperationSource` dataclasses. Both invoke the identical packaged WASM guest.

Each source supplies:

| Field | Meaning |
|---|---|
| `scope` | Exact `route`, uppercase HTTP `method`, `side` (`request`/`response`) and operation `version`. Both sources must match. |
| `kind` | `declared` for supplied schema JSON, or `observed` for captured JSON. Mixed provenance kinds abstain. |
| `provenance` | Caller-supplied review/capture revision; retained independently for both sources. |
| `body` | The complete schema or observation as a JSON string. |
| `complete` | Explicit capture completeness. False produces partial coverage with no findings. |

For example, comparing `{"type":"string"}` to
`{"type":["string","null"]}` as supplied declarations is compatible for a
request (new acceptance is wider), and breaking for a response (new output may
be null). The fixture tests execute both directions and nested variants.

## Interpret the result

`coverage` is `complete`, `partial` or `unsupported`, with explicit `gaps`,
`requiredness` and bounded `findings`. Each finding includes an escaped native
`path`, `pathKind`, change kind/class, directional `compatibility` and both
provenance labels. Declared item shape paths use `[]` and `pathKind: schema`;
observed homogeneous array changes expand to concrete indices with
`pathKind: concrete`. A schema shape path is not a concrete repair selection.

The declared subset supports explicit scalar/object/array types, type unions,
`properties`, `required`, string/null `enum`, and single-schema `items`.
Unsupported keywords (including references, combinators and additional-property
constraints) return unsupported coverage with no findings. This is a closed
subset, not arbitrary JSON Schema execution. Requiredness is authoritative only
for declared sources. Optional additions are compatible; required changes,
type/nullability and enum changes follow request/response direction.

Observations always retain partial coverage: field presence does not establish
requiredness, optionality, allowed enum values or unseen variants. Their findings
have unknown declared compatibility. Empty/heterogeneous item evidence, duplicate
members, malformed/incomplete capture or exhausted work cannot fabricate a
finding. The implementation conservatively abstains for the entire comparison
when these gaps prevent a reliable operation comparison.

Bounds: each body <=1 MiB, input envelope <=2.2 MB, <=4096 inspected nodes,
inspection depth <=64, declared schema depth <=32, <=64 enum members, and work
admission >=16 times envelope bytes (default 200,000,000). Invalid or unavailable
SDK comparisons return unsupported/unknown with no findings. Scope mismatch
never reads another operation's baseline. Baselines and review labels are
host-supplied; the comparison cannot independently establish their authority.

See [contract §10.59](/agindow-runtime-contract.md#1059-planned-verified-repair-and-api-change-coverage-expansion)
and [statistical guard](STATISTICAL.md) for the separate window-based detector.
