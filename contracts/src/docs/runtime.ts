import { CONTRACTS } from '../index.js';
import type { DocPage, DocSection } from './model.js';
const page = (slug: string, title: string, sections: readonly DocSection[]): DocPage => ({
  family: 'api-reference', version: 'v1', slug, path: `/docs/api/${slug}`, title, navTitle: slug === 'guard' ? 'Execute a tool call' : slug === 'recovery' ? 'Health, audit and outcomes' : title, published: true, sections,
});
export const guardExample = {
  tool: 'vendor.read', connection: 'conn_example', operation: 'read', args: {}, mode: 'observe',
} as const;
export const runtimePages: readonly DocPage[] = [
  page('guard', 'Execute a tool call — POST /v1/guard', [
    { heading: 'Execute a configured tool', text: 'Send an explicit JSON execution envelope to your own guard host. Use http://SIDECAR_HOST:8789, replacing SIDECAR_HOST with your configured local sidecar hostname; a Unix socket is also supported. A configured connection selects the upstream tool and its credentials; callers cannot supply arbitrary upstream URLs or authorization headers. This is a separate execution host from the read-only MCP interface. Managed guard execution is not available.' },
    { heading: 'Request fields', text: 'Use Content-Type: application/json. Unknown top-level fields are rejected. Required fields have no implicit mode or operation default on HTTP.', table: { columns: ['Field', 'Type / presence', 'Meaning / constraint'], rows: [
      ['tool', 'string; required', 'Configured tool name; 1–256 UTF-8 bytes, no control characters or slash.'],
      ['connection', 'string; required', 'Authorized connection handle; same identifier bounds as tool. Not a credential.'],
      ['operation', 'read | write | unknown; required', 'Must match the configured route. Treat unknown as a write for idempotency.'],
      ['args', 'JSON value; required', 'Upstream request arguments; at most 8 MiB of encoded JSON.'],
      ['mode', 'observe | repair | strict; required', 'Policy for both inspection phases; observe does not mutate or block.'],
      ['idempotency_key', 'string; required for write/unknown', '1–256 UTF-8 bytes, no controls; forwarded as Idempotency-Key. The upstream must enforce deduplication.'],
      ['metadata', 'object; optional', 'Defaults to {}. Does not grant authority or configure the detector.'],
      ['durability', 'reserved; omit', 'A non-null value is rejected. No durability service is selected through this request.'],
    ] } },
    { heading: 'Try a configured local read', text: 'Requires a running sidecar with conn_example/vendor.read already bound to a read operation. Replace both handles with your configured values. The response body comes from that upstream; there is no universal AGINDOW success JSON wrapper.', example: { language: 'bash', code: `curl -i http://SIDECAR_HOST:8789/v1/guard \\\n  -H 'Content-Type: application/json' \\\n  --data '${JSON.stringify(guardExample)}'` } },
    { heading: 'Successful or passed-through response', text: 'The upstream HTTP status, content type and original body are returned when no policy halt applies. A vendor 4xx or 5xx can therefore be a completed upstream outcome. Inspect X-Agindow-Status separately from HTTP status. This illustrative 200 body belongs to a fictional upstream, not a fixed AGINDOW response schema.', example: { language: 'json', code: '{"id":"example-record","amount":2000}' } },
    { heading: 'Diagnostic headers', text: 'Headers describe this call, not a protection guarantee. They may be absent when the host rejects a request before execution. Do not require numeric latency when the clock is unavailable.', table: { columns: ['Header', 'Meaning'], rows: [
      ['X-Agindow-Status', 'ok, repaired, degraded, bypassed or blocked. Current execution hosts do not apply selective repair.'],
      ['X-Agindow-Mode', 'Resolved observe, repair or strict policy when supplied.'],
      ['X-Agindow-Audit', 'Artifact ID, not a URL or promise of durable retention.'],
      ['X-Agindow-Drift-Class', 'shape, semantic or unknown; present when drift evidence is available.'],
      ['X-Agindow-Drift-Side', 'request, response or both; identifies which inspection phase found drift.'],
      ['X-Agindow-Latency-Ms', 'Active guard overhead excluding upstream waits; numeric locally or unavailable.'],
      ['X-Agindow-Latency-Clock', 'Clock source used by the remote execution host; may be unavailable.'],
      ['X-Agindow-Outcome', 'Relative retained-outcome URL on a completed response halt. Read it; do not blindly replay a write.'],
      ['X-Agindow-Upstream-Status', 'Original upstream status on a local sidecar response halt.'],
      ['X-RateLimit-Limit / Remaining / Reset', 'When emitted by an authenticated remote host: allowance, remaining count and reset Unix seconds.'],
      ['X-Agindow-Quota-Scope', 'isolate when emitted; limits are local to an execution instance, not a global quota guarantee.'],
    ] } },
    { heading: 'Policy refusal: HTTP 409', text: 'An AGINDOW policy refusal has X-Agindow-Status: blocked and exactly six string fields below. repair uses drift_blocked; strict uses drift_halted. Observe never produces a policy block. A request halt means no dispatch. A response halt means the upstream already completed; inspect the retained outcome. An upstream 409 alone does not establish an AGINDOW policy refusal.', example: { language: 'json', code: JSON.stringify({ error: 'drift_blocked', tool: 'vendor.write', drift_class: 'semantic', drift_id: 'aud_example', hint: 'Upstream completed. Inspect the retained outcome; do not blindly replay a write.', audit: '/v1/audit/aud_example' }, null, 2) } },
    { heading: 'Request validation, capacity and transport errors', text: 'When the guard rejects a request before execution, it generally uses {"error":"code"}; framework limits or origin refusals may have a plain or empty body. Upstream bodies use the vendor format. Do not assume every non-2xx response is JSON or safe to retry.', table: { columns: ['HTTP / code', 'Meaning', 'Next action'], rows: [
      ['400 invalid_request', 'Malformed JSON, unknown field, invalid/mismatched route operation, missing idempotency key or unsupported durability.', 'Fix the envelope before retrying.'],
      ['401 unauthorized', 'Connection/tool not admitted locally, or remote credential missing/invalid.', 'Check the handles and credential; do not switch tenants to retry.'],
      ['403', 'Local Host/Origin refused, or remote authorization/plan restriction.', 'Use the intended local origin or resolve access with the host owner.'],
      ['404 not_found', 'Unknown route or missing retained evidence.', 'Check the path; a missing receipt does not prove non-execution.'],
      ['409 drift_blocked / drift_halted', 'Known policy refusal; distinguish request and completed response.', 'Inspect feedback and retained outcome; do not blindly replay.'],
      ['413 invalid_request or framework body limit', 'Envelope exceeds 8 MiB + 64 KiB, or arguments exceed the inspection bound.', 'Reduce the request; do not silently split a logical write.'],
      ['429 rate_limited', 'This remote execution instance has reached its request limit.', 'Wait for the indicated reset if present; preserve the same logical idempotency key.'],
      ['502 internal', 'Sidecar could not obtain/read an upstream response.', 'Outcome may be uncertain. Reconcile writes before retrying.'],
      ['503 internal / identity_unavailable', 'Guard or authentication unavailable.', 'Use an explicit SDK fallback if configured, with the same upstream-enforced key for writes.'],
    ] } },
    { heading: 'Limits and retry safety', text: 'The statistical guest is default; configure windows to detect. Configure detection separately; selecting a mode is insufficient. Response inspection requires a complete body within the configured size limit; streams, encoded or incomplete evidence can bypass inspection. A malformed whole body is not repaired. The local vendor timeout is 30 seconds. Client timeouts do not cancel an already completed vendor write. SDK remote helpers default to five seconds and require a caller-supplied fallback. Audit and outcome records have fixed retention limits and do not provide durable replay.', links: [{ label: 'Recover an outcome', href: '/docs/api/recovery' }, { label: 'Remote SDK calls', href: '/docs/sdk/remote' }, { label: 'Modes', href: '/docs/modes' }] },
  ]),
  page('audit-artifact', 'Audit artifact schema', [
    { heading: 'Versioned evidence record', text: 'The complete JSON Schema below defines the audit response returned by the execution and MCP evidence APIs. Required nullable drift and patch fields distinguish absent evidence from a missing field. proof.write_integrity is an evidence claim, not a consequence of HTTP success; false means write integrity has not been established. Credential values do not belong in metadata.', schema: JSON.stringify(CONTRACTS.audit_artifact, null, 2) },
    { heading: 'Read a record', text: 'id and ts identify the call; operation and mode identify its frozen policy; drift describes classification, side and optional changes; decision names the call outcome. patch is null when nothing changed, otherwise applied and byte_identical describe the patch. proof records contract_version, verified_against and write_integrity, plus an optional idempotency_key. confidence, hypothesis, detail, note and metadata are optional. Missing retained evidence is unavailable, not proof of a clean call.', links: [{ label: 'Retrieval and retention', href: '/docs/api/recovery' }] },
  ]),
  page('recovery', 'Health, audit and retained outcomes', [
    { heading: 'Choose the same execution host', text: 'These are sidecar/execution API routes, separate from MCP fetch_audit. Read them from the host that handled the call. Local socket access provides local authority; remote archive reads require the same credential used for execution and current authorization. Never follow an outcome URL to a different origin with credentials.' },
    { heading: 'GET /healthz', text: 'No body required. Sidecar response: {"status":"ok","instance":"example","registered":true,"calls":0}. This reports process health and a local call counter, not detector readiness, upstream health or write safety. An authenticated remote execution host may return only status and the default engine name.' },
    { heading: 'GET /v1/registration', text: 'Sidecar only. No request body. Returns instance (string), routes (an array such as ["conn_example/vendor.read"]) and default_engine (string). The statistical guest is default; configure windows to detect. Lists configured local connection/tool handles; it neither registers a workload nor grants authority. Do not expose a loopback sidecar as a public credential service.' },
    { heading: 'GET /v1/audit/{id}', text: 'Use the ID from X-Agindow-Audit or the audit URL in policy feedback. Returns an application/json audit artifact when retained: id, ts, tool, operation, mode, drift, decision, patch and proof, with optional confidence and metadata. See the schema reference for the full versioned shape. Not every streaming or bypass call has a retrievable artifact.', links: [{ label: 'Audit artifact schema', href: '/docs/api/audit-artifact' }] },
    { heading: 'GET /v1/outcome/{id}', text: 'Use X-Agindow-Outcome after a response halt. Returns the retained original upstream status, headers and body with X-Agindow-Retained-Outcome: true. The HTTP status can be an upstream error. Retrieval does not execute the operation again. A request halt has no completed upstream outcome.', example: { language: 'bash', code: 'curl -i http://SIDECAR_HOST:8789/v1/outcome/aud_example' } },
    { heading: 'Missing or unavailable evidence', text: '404 means missing, expired, evicted, not captured, wrong host, or inaccessible evidence; it is not proof that a write never happened. The sidecar may return 503 internal when archive access fails. An unauthenticated remote read can return 401; remote credential mismatch or lost authorization is concealed as 404. Error bodies can be plain text. Retention is process-local: sidecar FIFO up to 128 entries / 16 MiB; remote host up to 64 entries / 16 MiB. Restarts and instance changes lose access. Reconcile uncertain writes with the vendor using the original idempotency key.' },
  ]),
];
