# Native CI latency gates

Use these gates when changing relay native code or its performance tooling.
They measure synthetic native fixtures on the executing machine. They do not
qualify the SDK/WASM headline, network latency, live traffic, or production.

Run from the repository root with Node and the repository Rust toolchain:
```sh
node cells/cell-relay/perf/run-regressions.mjs
node cells/cell-relay/perf/run-availability.mjs
```
The regression runner exports the exact Git revision in `baseline.v1.json`,
then builds and measures baseline and candidate sequentially with the same
runner and toolchain. Allow enough disk space for two release build trees.
The source baseline is committed as an immutable revision, rather than timing
numbers measured on unrelated hardware. Candidate sources include local edits;
CI uses its checked-out revision. Run without competing CPU workloads.

Every existing Criterion harness is listed in the manifest. CI tests require
new Criterion harnesses to be added there. Each suite uses 100 samples, 1 s
warm-up and 3 s measurement. The checker compares mean point estimates in
nanoseconds, allowing exactly 10% and failing above it. Empty, incomplete,
malformed or mismatched case sets fail. Output directories are cleared before
each run, so old measurements cannot rescue a failed run. Custom quantile
harnesses remain separate from this Criterion gate.

The independent availability runner retains its stricter contract §10.10
policy: bypass mean upper 95% confidence bounds must be <10,000 ns and faster
than the normal inspection mean lower bound.

The regression job runs independently of the other relay checks and Turbo.
`target/native-regressions/` contains environment metadata, both Criterion
report trees, and `report.json` after a complete comparison. CI retains these
reports for 14 days, including partial measurements when execution fails.
Artifacts and reported job status are scoped to the exact run attempt, so a
rerun cannot report an earlier attempt's passing measurements.
Build trees are not uploaded. A separate default-branch `workflow_run`
workflow posts the result and measurements to the associated PR, including
fork PRs; no PR code executes with its comment-writing permission. The reporter
must land on the default branch before GitHub can trigger it. Missing reports
are reported as unavailable, never as a passing comparison.

To update the baseline, choose a reviewed commit containing all manifest
harnesses, change the full revision in `baseline.v1.json`, run both gates and
review their retained results. Never update it automatically after a failure.
A deliberate case rename/addition needs a compatible reviewed baseline; a
mismatched set blocks CI. Review fixture/harness changes for equivalent work
because identical case names alone cannot prove equivalent workloads.

A 10% point-estimate threshold can fail due to shared-runner noise. Inspect
raw confidence intervals and rerun on an idle runner before drawing a causal
conclusion; the gate does not silently relax its threshold or ratchet its
baseline. See and the
.
