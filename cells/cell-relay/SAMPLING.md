# Explicit call-level array sampling

`agw_stats::windows::sampled` is an opt-in local library, governed by runtime
contract the runtime contract. Object-only stores
keep their original JSON Pointer behavior. There is no default guard, profile
export/refresh or array repair integration.

A `Selection` contains the normal the `Scope`, an `array` pointer from the
response root, an `item` pointer relative to an item object, a `Policy`, and an
explicit numeric `Projection`. For example, `array: "/orders"` and
`item: "/total"` selects one order's total. Both pointers traverse object
properties only and must start with `/`; v1 does not select a root array.
Escaped names, empty names, literal `*` and numeric property names work; wildcards and recursive traversal do not. Every item must be an
object. Null/scalar/array items reject even when unselected; selected container
leaves and traversal through arrays also reject.

```rust,ignore
use agw_stats::windows::{Projection, sampled::{Draw, Policy, Selection, Store}};

let selection = Selection {
Scope, // Exact tenant/connection/vendor/operation/side/field/segment/window/generation.
Array: "/orders".into,
Item: "/total".into,
Policy: Policy { version: "orders-v1".into, max_items: 1000 },
Projection: Projection::Disabled,
};
let mut store = Store::new(limits)?;
store.record(&[Draw { selection: &selection, entropy }], &inspection, at)?;
// Publish explicitly after the baseline floor; collect fresh live calls.
store.publish(&selection)?;
```

Every admitted call supplies one opportunity for each selection, regardless of
array length. A 1,000-item call counts once toward a 20-call floor. Missing/null/
empty arrays return `NoOpportunity` and admit nothing; a selected object's absent
field is a missing opportunity, while a null leaf remains null. Any unavailable
or invalid selection rejects the entire batch. Submit a call only once.

The fixed `uniform-call-v1` algorithm takes caller-supplied uniform u64 entropy.
For n items, words below `2^64 mod n` return `EntropyRejected`; accepted words
select index `entropy mod n`. This avoids modulo bias without a retry loop or
random-provider dependency. The host supplies fresh independent uniform words;
constant words are useful only for controlled tests/replay. Equal weighting of
calls does not give equal weighting to all items across calls of different sizes.
The sampler cannot establish traffic independence or representative populations.

Each policy version, maximum length, selector pair and scope has separate
windows/readiness/baselines. Projection is immutable within that identity. The
private canonical tuple key must fit 's 4096-byte scope-component limit.
`inspection_nodes` bounds complete validation plus pointer-search steps and array
item eligibility visits across the batch. Inspection text/depth limits and the
normal scope/sample/batch/retained-byte quotas apply. Failed admissions preserve
active references; explicit publication/advance/removal retain semantics.
Externally held snapshots and allocator slack are outside retained-byte accounting.

Snapshot `selection` and `unit` retain policy and `EligibleCall` metadata.
`baseline`/`live` provide immutable descriptive windows, exact strings/numeric
tokens and caller timestamps. Only explicit safe numeric authorization enables
unit projection. No selection permits repairs.

Before using `snapshot.evidence(Assumptions {... })`, the host must attest to
`independent_exchangeable_calls` and `comparable_populations`. Either false returns
`UnsupportedAssumptions` with no statistical report. The returned adapter offers
`completeness`, `coercion`, `charset`, `enumeration`, and `unit`, each requiring the
predeclared global hypothesis-family size. These wrap unchanged and expose
Materiality decisions. Insufficient detector denominators produce no tests.
Completeness counts admitted calls; enum counts string-bearing calls; units count
safe numeric calls. Descriptive windows alone do not establish significance.
Seasonal selection and comparisons remain host responsibilities; this store does
not pool seasonal windows or compare separate snapshots implicitly.

Run `cargo bench -p agw-stats --bench array_sampling` for native local costs on
0/1/1000/4096-item fixtures. It measures full duplicate-free validation, bounded
extraction and one-field cold admission. Parsing, fixture setup and store creation
are excluded. Results are saved in `target/ti51-array-sampling.json`; this is a
local measurement, not a runtime latency guarantee or production qualification.

Local native release measurement (2,200 timings after 200 warmups per fixture):

| Array items | p50 | p99 |
|---|---:|---:|
| 0 (unavailable) | 0.500 µs | 0.667 µs |
| 1 | 0.875 µs | 2.292 µs |
| 1000 | 32.875 µs | 50.833 µs |
| 4096 | 123.583 µs | 157.833 µs |

These numbers cover the benchmark scope above on the local native host; they
are not an enforced budget. The JSON result includes the compiler/target details.
