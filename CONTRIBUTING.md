# Contributing

Thanks for helping make API drift visible and safe to repair.

## How this repository works

**This repository is a mirror.** The SDK and in-path runtime are developed in a
canonical private tree and published here so the source that can inspect, block
or modify API calls stays auditable. There is no separate publish flow from this
repo alone, and we cannot merge your PR directly onto `main` (histories differ;
`main` is overwritten on each export).

We still review and accept contributions here. When we accept a change, a
maintainer **ports it upstream with attribution**, and it lands here again on
the next export. We then close your PR with a note pointing at the release or
export commit. A **closed** PR does **not** mean rejected — check the closing
comment.

Practically:

- Small fixes to docs, examples and tests are welcome — open the PR.
- For anything non-trivial, **open an issue first** so we can confirm scope
  before you invest time.
- Do not paste secrets, customer payloads, vendor credentials or production
  traffic into issues, PRs, fixtures or logs.

## Development

- Requirements, commands and the clean-clone gate: [BUILD.md](./BUILD.md).
- Tests come first. A behaviour change without a failing test is not ready.
- Keep one behaviour per test; never weaken an assertion to make a suite pass.
- Rust: no `unsafe`, no `unwrap()`/`expect()` in library code, propagate with
  `?`. Formatting and clippy warnings are build failures.
- TypeScript: strict, no `any`, no non-null assertions, type-only imports.
- Preserve byte identity. Only the explicit repair path may build a new object.
- The in-path engine never throws its own errors, never blocks a read and never
  gains a synchronous dependency on another service, a database or a model.

## Scope

In scope: the eleven relay crates, WASM guest, hosts, Node/Python SDKs, public
contracts and synthetic evaluation fixtures.

Out of scope for this repository: the shared corpus, profile generation, hosted
registry and control plane, Cortex, dashboard backend, identity, billing and
operations. Pull requests that add those will not be accepted.

## Security

Do not report vulnerabilities in a public issue. Follow
[SECURITY.md](./SECURITY.md).

## License

By contributing you agree your contribution is licensed under Apache-2.0, the
same licence as this repository. AGINDOW, the word mark and the logo are
trademarks; see [NOTICE](./NOTICE).
