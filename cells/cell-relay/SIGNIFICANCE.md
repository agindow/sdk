# Detector significance adapters (TI-50)

`agw_stats::significance` computes significance from raw windows and connects it
to TI-19's materiality interface. Original detector APIs remain descriptive and
unchanged. Contract §10.24 defines the methods and caller obligations. This is a
standalone library; TI-46 owns guard execution. No runtime is enabled by default.

## API and local gates

| Entry point | Inputs | Features tested | Denominator |
|---|---|---|---|
| `unit` | Two finite numeric slices | Magnitude ECDF | All numeric observations |
| `enumeration` | Repeated baseline strings and observed timestamped strings | Every pooled-union category | All string observations |
| `completeness` | Two slices of optional lossless `Value` references | Non-null + missing/null/zero/empty string | All opportunities |
| `coercion` | Same field windows | Six non-null types | Non-null values |
| `charset` | Same field windows | Seven character classes + four formats | Strings |

Each entry point takes `(path, baseline, observed, total_tests)`. The enum baseline
is **occurrences**, including repetitions, rather than a deduplicated vocabulary.
The adapter derives the vocabulary for TI-16 and retains both window counts for
significance. It tests known categories too, including cases where only their
frequency changes; the original enum detector still decides whether a novel
category is a material finding.

`Report<D>` owns the original detector result in `detection()`, a `sufficient()`
flag and every tested feature in `tests()`. Tests expose typed hypothesis, method,
side sample counts, normalized effect, unadjusted p-value and `is_finding()`.
`report.decision(index, &floors, tenant)` returns:

- `Some(Decision)` for a local finding, after TI-19 significance/materiality gates.
- `None` for an original detector non-finding, even if its p-value is tiny.
- An error for an invalid index or invalid TI-19 input.

Unavailable reports have `sufficient() == false` and no tests/p-values. Check this
before indexing. At least 20 eligible observations on **both** sides are required;
for type/charset comparisons this means 20 non-null/string values respectively.
The unchanged descriptive result is retained even if the significance adapter
requires additional baseline evidence (notably enum sample counts).

## Methods and assumptions

Rate/category features use two-sided, probability-ordered Fisher exact tests on
success/failure counts. This conditions on table margins, enumerating every
feasible allocation with mass no greater than the observed allocation. It is not
a doubled one-sided test. Hypergeometric weights are normalized about their mode;
relative tie tolerance and a 1e-12 conservative rounding allowance prevent spurious
zero p-values. Equal degenerate rates return 1. See the
[SciPy specification and reference tables](https://docs.scipy.org/doc/scipy/reference/generated/scipy.stats.fisher_exact.html).

Unit comparisons compute the two-sample ECDF distance D on **absolute magnitudes**.
This avoids subtracting extreme finite numbers and accounts for tied values
exactly using integer CDF cross-products. Effect is D in [0,1]. P-value is bounded
by `min(1, 4 exp(-2 r D²))`, with `r = nm/(sqrt(n)+sqrt(m))²`, plus conservative
rounding allowances. This is our derivation using a triangle inequality and two
single-sample Massart DKW bounds. It is conservative; no sharp small-sample
asymptotic KS probability is claimed. The
[Wei/Dudley paper](https://arxiv.org/abs/1107.5356) states the single-sample bound
and explains limits of sharper two-sample formulas. Numeric effect differs from
TI-15's normalized Wasserstein metric; both the original local gate and TI-19's
new effect floor must pass. This is evidence of magnitude change, not proof of a
unit conversion or a numeric repair proposal.

For enum tests, the category set comes from the pooled union, invariant to group
label permutations when conditioning on pooled category counts. Testing all union
categories avoids treating an observed-only novelty selection as a predeclared
single hypothesis. Fixed field features are all tested even when unchanged.

Interpretation requires independent exchangeable observations from the same null
distribution within each scope/seasonal comparison. Callers own valid sampling and
immutable input selection. Correlated calls, adaptive baselines, repeated peeking
and mixing seasonal populations require a separately justified design; these APIs
do not infer effective sample size or verify independence. Production false-positive
calibration and TI-19's real replay remain pending.

## Hypothesis families and bounds

Pass the **predeclared total** across all planned fields, detectors and seasonal
comparisons. Include hypotheses that yield no finding. Local minima are 1 unit,
5 completeness, 6 type, 11 fingerprint and the full union cardinality for enum.
The adapter rejects a total below its local minimum or outside 1–4096. Hosts must
sum all local families; independent calls cannot discover one another's tests.
For enum families with unknown future vocabulary, predeclare a conservative upper
bound (up to 512/category comparison), rather than counting only new categories.
TI-19 applies Bonferroni **once** to the returned unadjusted p-value. Changing
`total_tests` cannot change a feature's computed p-value.

Every side is capped at 4096 samples; enum sides are capped at 256 unique values.
Text values/number lexemes are capped at 4096 bytes with a 1 MiB aggregate input
budget. Paths are capped at 1024 bytes for TI-19 compatibility. Validate inputs
before allocating statistical-test work. Invalid/oversized input returns a local
error for future guard containment; no I/O, audit mutation, payload rewriting,
policy execution or notification delivery occurs.

```sh
cargo test -p agw-stats --test significance_math --test significance_adapters --test significance_limits
```
