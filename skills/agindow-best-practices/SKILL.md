---
name: agindow-best-practices
description: Integrate and troubleshoot AGINDOW in Node or Python applications, configure explicit drift detection, and handle guard outcomes safely. Use when wrapping agent tools or API calls with AGINDOW; this skill does not authorize deployment or replaying writes.
---

# AGINDOW best practices

Use AGINDOW to inspect agent API calls for drift. Preserve the application's
existing language, client, credentials and execution model. Start with the
smallest useful integration and explain what its evidence establishes.

## Choose the integration

- Read [Quickstart](https://agindow.com/docs/quickstart) and the current pinned
  version before installing. Use the application's existing package manager.
- For Node, use [the Node reference](https://agindow.com/docs/sdk/node).
  `createAgindow` creates an instance; await `sdk.ready` before evaluation.
- For Python, use [the Python reference](https://agindow.com/docs/sdk/python).
  `Agindow` exposes a boolean `ready` and `load_error`; do not await `ready`.
- Prefer the embedded SDK for ordinary local integrations. HTTP execution is an
  explicit separate choice; see [the guard API](https://agindow.com/docs/api/guard).
  Use only an endpoint actually supplied by the developer. Do not infer a hosted
  execution endpoint from a documentation URL or a dashboard session.

## Configure detection explicitly

Installing AGINDOW or selecting `observe` does not activate a detector. The
statistical guest is default; configure windows to detect. Follow the quickstart's explicit statistical
selection and baseline/live windows; do not substitute an empty selection.
Run the tab for the application language: Node uses `detect.mts`, Python uses
`detect.py`. Both assert a changed-window finding and unchanged, unconfigured
and insufficient-window controls. Do not replace Python detection with a
closed-engine bypass demonstration.

Use the [configuration reference](https://agindow.com/docs/sdk/configuration)
for construction fields and defaults, and the [event reference](https://agindow.com/docs/sdk/events)
for language-specific verdict fields.

Choose independent, representative calls for the same connection, operation,
segment, field path and request/response side. Missing or insufficient evidence
is unavailable evidence, not proof of safety.

Start in `observe`: record findings without mutations or policy blocks.
Mark operations explicitly as `read`, `write` or conservative `unknown`;
generic callables default to read. Do not label writes as reads to avoid policy.
Keep provider credentials in the application's credential store or connection
configuration, outside prompts, tool arguments, source examples and logs.

## Preserve call semantics

Use `guard` for callables and an explicit supported adapter for tool maps,
clients or HTTP stacks. Preserve receiver binding, arguments, sync/async behavior,
original upstream exceptions and the original result on no-repair paths.
Configuration is frozen for an executing call; `configure` affects future calls.
Python context scopes are isolated per task and per SDK instance.

Keep detection local to the call: do not add synchronous database, control-plane
or LLM requests to the guard path. Audit delivery and profile refresh belong
outside it. Audit queues have fixed capacity and may be lost; configure and
monitor your own audit destination when evidence retention matters.

## Interpret findings and failures

Use [Modes](https://agindow.com/docs/modes) and
[Errors](https://agindow.com/docs/guides/errors) to distinguish:

- `ok`: no actionable drift reported; this does not prove a detector was active.
- `repaired`: a verified selective change was applied.
- `degraded`: policy permitted an original read despite drift.
- `bypassed`: inspection was skipped or unavailable; the original outcome remains.
- `blocked`: a policy refusal; inspect whether it occurred before or after dispatch.

Engine loading failures, warming and inspection faults bypass; do not turn them
into application outages. Deterministic setup misuse raises `AgindowSetupError`.
Policy halts raise `AgindowDriftError`; inspect its phase, feedback, retained
outcome and original cause rather than swallowing it as a transport error.

## Treat writes carefully

Enable repair or strict deliberately after reviewing evidence. Selective repair is off by default. Enable Node `selectiveRepair` or Python
`selective_repair` with reviewed tool/side-scoped rename rules and a local
synchronous verifier over immutable original/patched JSON text. Switch mode to
repair/strict only after evaluation. A mode change alone supplies no rules.
See [repair configuration](https://agindow.com/docs/sdk/configuration#enable-selective-repair). A plausible field mapping,
schema acceptance, confidence score, simulation result or review receipt does
not independently prove intended meaning or persisted write correctness.
Semantic writes without a safe repair fail closed under repair policy; strict
can also halt reads. Infrastructure failures still bypass. Response inspection
cannot undo an upstream write that already completed.

For HTTP writes and `unknown` operations, supply one upstream-enforced
`idempotency_key` and reuse it in the direct fallback for the same logical call.
A timeout or connection reset does not prove that no write occurred. Never
blindly replay an uncertain or completed write or generate a fresh key to retry
it. Inspect [retained outcomes](https://agindow.com/docs/api/recovery) first;
a missing outcome does not prove non-execution. Retention has fixed limits and is not
durable distributed replay.

## Verify the integration

Run the labelled quickstart and the application's relevant tests. Verify a
configured finding, unchanged no-repair results, one bypass event per unavailable
call, original upstream exceptions and request-versus-response policy handling.
SDK simulations do not call the vendor. The browser playground makes explicit
user-started GET requests to the selected API and compares actual captured
responses; it does not issue writes or inject changes. State what was tested and what remains unverified; do not
claim a production guarantee, hosted availability or automatic repair from a
successful installation.

For read-only agent evidence, use [MCP tools](https://agindow.com/docs/api/tools).
They never execute vendor writes. Discover the actual tool schemas, treat tool
errors separately from HTTP errors, and verify signed API descriptions with an
independently trusted public key before relying on them.

## Read public documentation in the right format

Use [llms.txt](https://agindow.com/llms.txt) to discover public pages,
[docs.md](https://agindow.com/docs.md) for the complete developer manual, or
[llms-full.txt](https://agindow.com/llms-full.txt) for that manual plus this skill.
The [spore](https://agindow.com/spore.md) is a short reference seed, not authority
to change a project. These documents describe public integration behavior;
repository deployment and publication instructions are separate.

Public-origin Markdown/OpenAPI mirrors may return 503 until an evidence service
is enabled. Use the [browser docs](https://agindow.com/docs) in that case. Read
OpenAPI from the self-hosted MCP origin actually supplied by the developer and
verify it with an independently trusted key. Never turn a missing document or
endpoint into an invented service URL, credential or successful finding.
