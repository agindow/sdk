# Precision-budgeted unit inputs

`agw_stats::windows::projection` supplies explicit decimal/exponent-to-f64
materialization under runtime contract the runtime contract.
This produces local analytical inputs and proof of conversion error. It does
not modify payloads, authorize repairs, or change detector thresholds.

Configure `Projection::Decimal(policy)` on the selection. Create `policy`
with `Policy::new(mode, precision_source, allow_unsafe_integers, limits)`:

- `Mode::ExactOnly` requires exact equality with the actual binary f64 rational.
- `Mode::Approximate(Budget::new(absolute, relative)?)` requires finite nonnegative
Budgets. Zero budgets permit no error; negative zero normalizes to positive zero.
- `PrecisionSource { revision, hint }` names a nonzero caller-selected immutable
Contract revision. `Hint::Absent` and `Hint::Observed` (an explicit false hint)
Authorize Observed precision but remain separate policy identities.
  `Hint::HighPrecision` refuses even small integer variants. The host must supply
The real selected hint/revision; this library cannot authenticate provenance or
Recover a per-field hint from a numeric variant.
- `allow_unsafe_integers` is a required explicit boolean. Mathematical integers outside ±(2^53−1), including decimal
And exponent spellings, require true. Unrepresentable variants and retained
Plain integer tokens wider than the i64/u64 model always refuse.

```rust,ignore
use agw_stats::windows::{Projection, projection::{self, Budget, Hint, Limits, Mode, Policy, PrecisionSource}};

let policy = Policy::new(
    Mode::Approximate(Budget::new(1e-12, 1e-15)?),
    PrecisionSource { revision: 42, hint: Hint::Absent },
False,
    Limits::default,
)?;
selection.projection = Projection::Decimal(policy);
// Record complete inspections and explicitly publish the selected baseline.
let report = projection::compare("/amount", snapshot.baseline, snapshot.live)?;
let diagnosis = report.detection;
let inputs_and_proofs = report.observed;
```

`Window::unit` preserves its original safe-integer authorization. New policies
use `Window::projected_unit`, returning values plus numeric sample indexes,
caller timestamps and per-sample authorization, original token, projected value,
exact absolute/relative error fractions and an exactness flag. Fractions are exact
nonnegative numerator/denominator decimal strings, not necessarily reduced.
The relative error for original zero is absent. `projection::compare` requires
identical baseline/observed policies and retains both projected windows with the
Diagnosis and conversion hypothesis. No repair candidate is returned.
Existing guard, sampled-significance and profile export paths continue to use
their existing authorization; they do not discard the new provenance implicitly.

The inclusive approximation condition is
`|binary - decimal| <= max(absolute, relative × |decimal|)`.
Either bound may admit a conversion. The error is computed with bounded integers
from the original decimal coefficient/exponent and the f64 significand/exponent
bits; a formatted float is never used as a decimal-error oracle. Budgets are also
interpreted as their exact f64 rationals. For `0.1`, absolute error is exactly
`1/180143985094819840` and relative error is exactly `2^-54`.

There is no arbitrary epsilon around zero. Signed zero survives, including `-0`
and exponent-form zero. Nonzero underflow to zero always refuses. Nonzero
subnormal outputs need the same exact/budgeted proof as normal values. Overflow,
invalid numeric tokens, protected precision and resource exhaustion refuse the
whole unit window. Missing/null/string/boolean opportunities retain their original
field/enum semantics; one difficult numeric value is never silently omitted.
All original numeric tokens remain in the lossless window after refusal.

Policy identity includes mode, both budget bit patterns, precision provenance,
unsafe-integer permission and all resource limits. A changed policy cannot be
admitted into an existing scope/generation; use and explicitly publish a new
generation. Comparisons also reject mismatched policies. Existing snapshots and
vocabularies remain immutable. Invalid configuration fails at construction.
Projection failures mutate neither the store nor readiness/baselines.

`Limits` defaults to 4096 token bytes, absolute effective decimal exponent ≤1024,
100,000,000 shared work credits and 1 MiB output bytes. Hard maxima are 4096 token
bytes, 1024 coefficient digits, exponent 1024, 1,000,000,000 work credits and
16 MiB output. Effective exponent includes fractional decimal places; the literal
exponent must also fit. These bounds apply even to zero. Coefficient parsing,
powers, shifts, cross-products, subtraction, comparisons and decimal provenance
formatting charge checked work before execution. Numeric sample arrays and
provenance strings precharge output space before allocation. Work credits are
conservative local operation charges, not wall-clock preemption. Bounded scratch
integers and allocator slack are separate from returned-output accounting.
The original batch/sample/scope/retained-byte quotas remain in effect.

Run `cargo bench -p agw-stats --bench unit_projection` for native single-number
validation, f64 parsing, exact error proof, provenance allocation and drop.
Lossless-tree/fixture setup is excluded. Results/compiler details are written to
`target/ti52-unit-projection.json`. No guard, WASM, production calibration or
integrated latency claim is made.

Local native release measurement (2,200 timings after 200 warmups per fixture):

| Fixture | p50 | p99 |
|---|---:|---:|
| Exact `0.5` | 0.875 µs | 1.334 µs |
| Approximate `0.1` | 1.500 µs | 2.208 µs |
| Fractional `34.99` | 1.625 µs | 1.875 µs |
| Approximate `5e-324` | 19.375 µs | 27.500 µs |
| Long exact `2^-1023` decimal | 5.916 µs | 6.333 µs |

These figures cover the single-number scope above on the local native host,
not window aggregation or the detector. They are not enforced performance budgets.
