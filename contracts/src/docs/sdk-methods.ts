import type { DocSection } from './model.js';
export const nodeMethods: DocSection = { heading: 'Method reference', text: 'Signatures below use the published TypeScript names. Guard options are optional; mark writes explicitly. Setup validation can throw AgindowSetupError before a call runs.', table: { columns: ['Method / input', 'Return / failure'], rows: [
  ['createAgindow(options?)', 'SDK instance. options: mode, selection, onEvent, buffer, audit and other typed configuration. Invalid setup throws AgindowSetupError.'],
  ['sdk.ready', 'Promise<void> that settles after loading; settlement alone does not prove detection was configured or loaded successfully.'],
  ['sdk.guard(fn, {tool?, operation?, mode?, failClosed?})', 'Same callable type, receiver, arguments and sync/async behavior. Original result or upstream exception; policy may throw AgindowDriftError.'],
  ['sdk.wrap(target, options?)', 'Wrapped callable, tool map/array, supported client or HTTP target. Ambiguous boundaries require an explicit adapter.'],
  ['sdk.toolAdapter / clientAdapter / httpAdapter(target, options?)', 'Explicit wrapped target of the original type. Unsupported shapes are setup errors; streaming inspection can bypass.'],
  ['sdk.configure(config)', 'void. Applies changes to future calls; executing calls retain their configuration.'],
  ['sdk.on(kind, listener)', 'Unsubscribe callback. kind: drift, repair, blocked, bypass or error. Listener faults do not affect upstream execution.'],
  ['sdk.recent()', 'Recently retained event metadata, limited by local capacity; not a durable audit archive.'],
  ['sdk.simulate(tool, {recorded, args?, partial?, contract?, mode?, operation?})', 'Simulation with status, driftClass, confidence, changes, byte_identical and wouldWrite. Never calls upstream. partial evidence bypasses.'],
  ['sdk.drainAudit(sink, limit = 64)', 'Promise<{delivered, pending, dropped, available}>. Requires explicit audit configuration; it sends records to your destination after the guarded call. Delivery failure remains visible as unavailable.'],
  ['guardRemote(original, envelope, options)', 'Promise<Response | fallback result>. Explicit HTTP execution; see Remote SDK reference for timeout, feedback and fallback behavior.'],
] } };
export const pythonMethods: DocSection = { heading: 'Method reference', text: 'Python uses snake_case configuration. Guard and wrap preserve the original callable type. It does not expose the Node adapter, simulation, CLI or drainAudit methods.', table: { columns: ['Method / input', 'Return / failure'], rows: [
  ['Agindow(**options)', 'SDK instance; keyword configuration includes mode, selection and on_event. Invalid setup raises AgindowSetupError.'],
  ['sdk.ready / sdk.load_error', 'Boolean readiness and retained loading failure. ready is not awaitable.'],
  ['sdk.guard(fn?, tool=..., operation="read", **options)', 'Decorator or wrapped callable with original sync/async behavior. Upstream errors propagate; policy may raise AgindowDriftError.'],
  ['sdk.wrap(fn, tool=..., operation="read", **options)', 'Wrapped callable; no automatic Python framework adapter.'],
  ['sdk.context(**options)', 'Sync/async context manager; scoped overrides for this SDK, isolated across tasks.'],
  ['sdk.configure(**changes)', 'None. Changes future calls.'],
  ['sdk.on(kind, listener)', 'Unsubscribe callable. Listener failures do not affect upstream execution.'],
  ['sdk.recent()', 'Tuple of recently retained events, limited by local capacity.'],
  ['sdk.close()', 'None. Releases the guest; subsequent guarded calls bypass.'],
  ['guard_remote / guard_remote_async(original, envelope, options)', 'RemoteResponse or fallback value, synchronously or asynchronously; explicit HTTP only.'],
] } };
export const sdkErrors: DocSection = { heading: 'Errors and outcomes', text: 'AgindowSetupError identifies deterministic setup misuse before execution. AgindowDriftError identifies a policy halt and exposes phase, verdict, retained outcome and feedback; preserve the original cause. Engine warming, loading failure and inspection faults emit bypass and preserve the original result or exception. A response-phase halt cannot undo a completed upstream write. A no-finding default call does not prove detection is active.', links: [{ label: 'Error meanings', href: '/docs/guides/errors' }, { label: 'Outcome recovery', href: '/docs/api/recovery' }] };
export { remoteSdk } from './remote-sdk.js';
