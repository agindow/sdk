# Embedded release-WASM measurements (, )

This harness invokes the production the `loadWasm`/`bindGuest` boundary and
's actual detector, significance, routing, policy and containment code.
No production SLA or Python/Worker latency claim follows from these fixtures.
the runtime contract freezes the budget matrix before measurement; records the
host/guest UTF-8 byte boundary used by the optimized run.

```sh
AGINDOW_WASM_BINDGEN=wasm-bindgen node cells/cell-relay/node/scripts/build.mjs
node cells/cell-relay/node/scripts/probe-statistical.mjs
node cells/cell-relay/node/scripts/bench-statistical.mjs
node cells/cell-relay/node/scripts/bench-bypass.mjs
node cells/cell-relay/node/scripts/check-statistical.mjs
```

Run without concurrent builds/tests. Each of 351 statistical cases has 1,000
warmups and 5,000 individually timed calls. All modes/operation kinds, all five
actual detector kinds, clean/material/suppressed evidence, both/request-versus-
response halt branches, 16/128-sample windows and 862-byte/64-KiB payloads are
predeclared. Small windows remain insufficient evidence, never fake suppression.
Branch counts and detector eligibility are validated independently of timing.

Timing includes host capture, transitions, copies, parsing, extraction, window
materialization, detectors, significance, routing, policy, diagnostic result
parsing/accounting and disposal. Setup-only window publication and network/vendor
latency are excluded. Normal calls must retain the original result reference;
request halts dispatch zero times, response halts dispatch once. Nearest-rank
p50/p99, mean/stddev and alternating paired guarded/control differences retain
negative differences. Raw samples and environment/compiler metadata are saved to
`target/wasm-statistical/report.json` before checking p50 <200 µs / p99 <2 ms.

`bypass.json` separately measures not-ready, fresh exhausted-budget, open-breaker,
missing/corrupt setup snapshots and warmed retired-guest paths. Each call emits
one bypass and invokes the original once. Fresh budget faults use an untimed
successful reset call between independent samples so an open breaker cannot
masquerade as budget exhaustion. First trap/retirement and setup are recorded
separately; a real Rust panic fixture traps once and the host never re-enters it.
Twenty release-guest setup/first-call runs report lifecycle variation. The full
Node boundary's warmed bypass mean target is <10 µs; native remains its
own gate and cannot qualify this boundary.

The checker rejects incomplete/non-finite/mismatched samples and decisions.
Regression thresholds are predeclared at +15% p50 / +25% p99 versus a matching
runtime/hardware/matrix baseline. Set `AGINDOW_PERF_BASELINE` to an independently
retained **qualified** report directory. No baseline is manufactured from a
failing run. CI restores its last qualified cached baseline and enforces both thresholds.
The first run is an explicit absolute-only bootstrap; only a run passing all
absolute gates may populate the baseline cache. The cache key fingerprints runtime/hardware/compiler/flags, so each new
environment must independently pass its absolute bootstrap. A changed fixture
matrix requires a cache-version review; incompatible restored reports fail. Mutation
tests exercise both absolute and relative threshold failures.

## optimization

 profiled the failing matrix before changing code. On the 64-KiB anchor
case, host capture (`captureBytes`, including its escape scan and UTF-8 encode)
was 28.3 µs p50 / 86.4 µs p99; each guest phase (one JS-to-WASM byte copy,
UTF-8 parse, projection and detector/significance work) was ~76 µs; request
routing and `close` were 0.04 µs; disposal added nothing measurable. The
dominant detector cost was numeric lexeme formatting: every numeric sample
allocated a temporary `String` in each of the five detectors and both phases.
All 5,000 samples per case are retained, and the worst case occurs mid-matrix,
so no cross-case lifecycle trend was observed. `Number::lexeme_len` computes the
lexeme length without allocating and is used by the profile/significance hot
paths; retained `Verbatim`/`Unrepresentable` lexemes still run the original
validation. On the same case, guarded p50/p99 fell from 791.87/2,069.50 µs to
151.04/325.71 µs; the small Enumeration tail fell from 57.17/3,160.21 µs to
29.50/49.88 µs. A proposed unescaped-string fast path in `agw-inspector` showed
no measurable gain and was removed rather than kept unproven.

The host boundary now projects captured JSON to UTF-8 bytes once and passes a
`Uint8Array` to the generated guest (`capture` still returns the string).
Escape-bearing strings still use `JSON.stringify`; surrogate-containing or
otherwise unrepresentable values are rejected exactly as before. The wasm
pointer/length ABI is unchanged, so Python and Node share the identical guest.
the runtime contract and wrapper record the decision.

## Retained local run — 2026-10-21 UTC

Node v22.21.1 / V8 12.4.254.21-node.33, Apple M2, Darwin 25.5.0 arm64;
Rust 1.99.1 (58cf8f9c3), release wasm32-unknown-unknown, no custom Rust flags,
wasm-bindgen 0.2.129. Guest size: 938,043 bytes; initial default load: 12.160 ms;
cold guest setup: 4.362 ms (reported separately from warmed calls).

All 351 cases have complete finite samples and valid actual branch counts.
The absolute statistical gate **passes**: maximum guarded p50/p99 are
156.333/558.958 µs, against p50 <200 µs / p99 <2 ms. The previously failing
report is retained beside it at
`target/wasm-statistical/retained/report-failed-60011cb0.json`
(SHA-256 `60011cb09b86e98dbfaab6f0a5c39b5a45387d7a546a5660b085b75da0d2a2f7`,
max p50/p99 859.375/3,398.542 µs) and was not resampled. Neither budgets nor
the matrix were relaxed.

The separate embedded bypass gate passes: mean µs are not-ready 8.469,
fresh budget 7.854, open breaker 7.387, missing setup snapshot 0.469, corrupt
setup snapshot 0.302, and retired guest 5.965. Trap entry/retirement and fresh
recovery setup remain separately reported. Default Node/Python SDK added-p50
results are 11.083/40.833 µs. These SDK/statistical runs use publication disabled;
publication-enabled latency and production behavior remain unqualified.

Raw statistical report SHA-256:
`e730cd64127c18d20202706e8c405fe60f264012cb812cf4ca5695a21209938d`.
The retained bypass report SHA-256 is
`74d2fc2fa5f4232c1de17cce9854f9f0fea3c2a46ee7de3d70283e5118de57ac`; the
focused host/guest breakdown is in
`target/wasm-statistical/retained/attribution-2026-10-21.txt`.
A local qualified baseline was retained in `target/wasm-perf-baseline`; the
regression checker accepts it, while pointing the checker at the retained failed
report rejects the run on the small-tail p99 ceiling. CI still bootstraps per
environment fingerprint. The raw samples stay in target/ and CI artifacts, not
source control. The source matrix and checker commands reproduce the evidence.
