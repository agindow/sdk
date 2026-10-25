---
name: guarding-nango-functions
description: Add or refresh AGINDOW's generated local drift gate in Nango createAction/createSync functions when drift protection is requested. Uses the installed offline generator and an explicit local contract; does not deploy or contact providers.
---

Use the Nango function builder's normal authoring workflow. Keep its existing
schemas, relative imports and `nango` execution context. Do not add npm imports
to an action or sync. The generator itself runs in the developer's Node runtime.

Obtain the caller's local generation input: version 1 with `contracts`, one
explicit file/tool/method/endpoint contract per file. See the package's
`examples/nango-contracts.json` for the format. A historical drift event or a
plausible field name does not establish identity or authorize a rename. If
confirmed identity, type or confidence evidence is missing, omit the rename.
Do not copy the demonstration's fictional payment evidence into a real contract.

Run `agindow report <project>` first for read-only discovery. Generate or refresh
with `agindow scan <project> --snapshot <local-contracts.json>`; use
`agindow codegen <local-contracts.json>` to inspect the exact generated block.
Use the generator's output unchanged. Do not reproduce or hand-edit its runtime.

The transformer recognizes imported `createAction`/`createSync`, their `exec`
context receiver, and literal HTTP method/endpoint calls. Unsupported files are
counted and left unchanged. A request check must precede its provider call;
placing all checks after the final call cannot protect a write. Response checks
run after the call and use the context's `ActionError`. Keep any final response
check after the final provider call in a multi-call function, and review which
calls the file-scoped contract actually matches.

Re-run the generator: the second run must report zero changed files. Compile
and run the project's existing offline tests. A deployed Nango dryrun requires
the user's connection and environment; do not invent its result or contact a
provider merely to validate a generated block.

State the limits: scalar shape/enum checks and explicit request sibling renames;
no statistics, learned invariants, automatic conversions, hosted audit or write
integrity proof. Repair changes parsed values, so raw byte identity is not
claimed. The generator adds no network call. `AGINDOW_DISABLED=true` is a
build-time opt-out when regenerating; it is not a live environment poll.
