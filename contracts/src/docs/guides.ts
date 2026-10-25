import { contract, decisions } from './model.js';
import type { DocPage, DocSection } from './model.js';
import { nodeExample, auditExample, observeExample } from './examples.js';
export { docsChannels, sdkMatrix } from './sdk-matrix.generated.js';
import { sdkMatrix } from './sdk-matrix.generated.js';
export const adapterMatrix = [
  ['Pipedream', 'Local clientAdapter(client.proxy); synthetic proxy transport fixture.'],
  ['LangChain', 'Local toolAdapter invoke and ToolCall fixture; batch and stream execution have not been tested.'],
  ['Nango', 'Local report and limited sandbox codegen; npm-forbidden sandboxes require codegen, not WASM installation.'],
  ['Mastra', 'Generic tool-shape adapter only where supported; end-to-end framework testing is unavailable.'],
] as const;
const related: DocSection = { heading: 'Continue', text: 'Keep configuration, call decisions and evidence separate.', links: [{ label: 'Modes', href: '/docs/modes' }, { label: 'The guard loop', href: '/docs/the-loop' }, contract('8-the-trust-ramp--modes-as-a-sales-instrument')] };
const guideLabels: Readonly<Record<string, string>> = {
  frameworks: 'Framework integrations', 'observe-first': 'Start in observe mode',
  'end-to-end': 'Collect audit evidence', errors: 'Error reference', troubleshooting: 'Troubleshooting',
};
const page = (slug: string, title: string, sections: readonly DocSection[]): DocPage => {
  const navTitle = guideLabels[slug];
  return { family: 'guides', version: 'v1', slug, path: `/docs/guides/${slug}`, title, published: true,
    ...(navTitle === undefined ? {} : { navTitle }), sections: [...sections, related] };
};
export const guides: readonly DocPage[] = [
  page('frameworks', 'Framework drop-ins', [
    { heading: 'Local adapter release matrix', text: `The Node package \`${sdkMatrix.node.name}@${sdkMatrix.node.version}\`, published on npm, supplies these adapters. Integration order is Pipedream → LangChain → Nango → Mastra. This order does not claim equal framework coverage or hosted partner adoption.`, table: { columns: ['Framework', 'Available local surface'], rows: adapterMatrix }, links: adapterMatrix.map(([name]) => ({ label: name, href: `/integrations/${name.toLowerCase()}` })) },
    { heading: 'Choose the actual boundary', text: 'Use `toolAdapter` for supported func/execute/invoke maps and arrays, `clientAdapter` for supported HTTP methods, and `httpAdapter` for Fetch/Express boundaries. Wrap selects a supported shape and refuses ambiguity before execution. Client storage and authentication helpers are not HTTP calls. Local SDK examples in the Node package exercise actual Pipedream proxy and LangChain tool executors without provider credentials.' },
    { heading: 'Nango sandbox gate', text: 'Use the separately tested report/codegen path for Nango sandboxes that forbid npm. Generated code has no credential lookups or imports. Only explicitly confirmed same-type request sibling renames are supported; the code has not been tested in a deployed Nango sandbox.' },
  ]),
  { ...page('observe-first', 'Adopt with observe', [
    { heading: 'Start with a read and observe', text: 'Install the published SDK as described in Quickstart, select observe explicitly, await ready, and wrap the boundary you own. The validated synthetic example below retains the exact result object and never enables a repair path. The default engine passes calls through without detecting drift. Configure a statistical detector with scoped baseline/live data windows; selecting observe mode does not configure a detector.', links: [{ label: 'Install the SDK', href: '/docs/quickstart' }] },
    { heading: 'Read candidates without applying them', text: 'Subscribe with `sdk.on` to drift, repair, blocked, bypass and error events, or inspect `sdk.recent()`. A proposed patch is not an applied repair. The second snippet injects a labelled synthetic candidate engine and asserts that observe retains the original object even when a replacement is offered. Observe never mutates even when a trusted engine proposes a replacement. A bypass means inspection was unavailable; it is not clean drift evidence.' },
    { heading: 'When to stay in observe', text: 'Do not switch based on candidate count or recovery rate alone. Require representative baseline/live windows, scoped operation identities, verified patches and violation-rate evidence. Missing evidence, cold baselines and unsupported captures need investigation. The default guest has no selective repair output; changing mode cannot supply it. Semantic writes remain protected in repair mode; strict requires explicit review handling.' },
  ]), snippets: [nodeExample, observeExample] },
  { ...page('end-to-end', 'End-to-end flow', [
    { heading: '1. Wrap a labelled local fixture', text: 'Install the published package from Quickstart, then run the validated ts/py examples below. The synthetic examples assert result identity and a deliberate bypass. They never contact a vendor or execute a live write.', links: [{ label: 'Quickstart', href: '/docs/quickstart' }] },
    { heading: '2. First evidence', text: 'The healthy default engine reports no drift because no detector was selected. The explicit loading-fault or closed-engine fixture produces one bypass event. Read recent metadata without treating either result as proof that an API is safe.' },
    { heading: '3. Audit record', text: 'The second tested snippet’s TypeScript tab configures an audit tenant and queue capacity, runs a synthetic read, and drains the actual WASM audit record into a labelled temporary in-memory destination. The Python tab retains identity and local events; draining the guest audit queue remains Node-only. Production hosts must save a record before acknowledging delivery. This fixture has no hosted retrieval URL; a full or lost queue cannot prove durable delivery.' },
    { heading: '4. Optional review', text: 'A review case requires admitted strict-policy evidence and an explicitly configured host review port. This example does not create one. A reviewer receipt describes a human decision; it does not execute a write or release a terminal idempotency key. Production review delivery remains unavailable.' },
  ]), snippets: [nodeExample, auditExample] },
  page('errors', 'Errors', [
    { heading: 'Runtime contract error taxonomy', text: 'These are the contract §7 error names and HTTP statuses. This is the runtime specification, not a claim that the MCP endpoint implements the guard envelope.', links: [contract('7-error-taxonomy')], table: { columns: ['HTTP', 'error', 'Meaning'], rows: [
      ['`400`', '`invalid_request`', 'Bad envelope or missing write idempotency key.'], ['`401`', '`unauthorized`', 'Invalid credential.'],
      ['`403`', '`plan_limit`', 'Plan ceiling.'], ['`409`', '`drift_blocked`', 'Write blocked by known policy.'],
      ['`409`', '`drift_halted`', 'Strict policy halt.'], ['`422`', '`contract_violation`', 'No known contract.'],
      ['`429`', '`rate_limited`', 'Guard-side throttle.'], ['`5xx`', '`internal`', 'Availability failure; explicit client fallback rules apply.'],
    ] } },
    { heading: 'Five decisions', text: 'Engine fault `bypassed` is different from policy `blocked`. A fail-open read can be `degraded`; a successful selective replacement is `repaired`.', table: decisions },
    { heading: 'SDK and administration errors', text: '`AgindowSetupError` is deterministic misuse before a call. `AgindowDriftError` carries policy feedback and preserves original exceptions as cause where available. A response halt may retain a completed outcome; inspect it before taking further action. Dashboard session, permissions, retention and validation failures belong to their own administrative contracts, not this runtime taxonomy.' },
  ]),
  page('troubleshooting', 'Troubleshooting', [
    { heading: 'Bypass', text: 'Await Node `ready`; check Python `ready` and `load_error`. Confirm explicit detector selection, tool scope, complete captures and work allowance. A retired guest requires a new SDK instance. Inspect the bypass event; do not infer protection from unchanged output.', links: [{ label: 'On error, bypass', href: '/docs/on-error-bypass' }] },
    { heading: 'Blocked', text: 'Read the structured drift hint, phase and retained outcome. Request refusal dispatches nothing; a response halt can follow a completed upstream write. Do not blindly replay it. Review classified evidence and upstream-enforced idempotency before any further write.' },
    { heading: 'Retention expired', text: 'Check the retention receipt or the storage system responsible for the archive. Missing or expired evidence is unavailable; it is not proof of no drift. Local queues have fixed capacity and may be lost on restart. Do not invent an audit record or reconstruct secret payloads from summaries.' },
    { heading: 'Cold baseline', text: 'Supply explicit comparable baseline/live windows in the supported scope. A warming or missing baseline cannot establish confidence. Keep observe until there is representative evidence; changing mode does not warm the baseline.' },
    { heading: 'Unavailable adapter', text: 'Check the local adapter matrix and use the explicit supported boundary. Unsupported shapes remain unchanged or produce a named setup refusal for ambiguity. Use Nango codegen only at its supported sandbox gate; a planned adapter has no install instructions.', links: [{ label: 'Framework matrix', href: '/docs/guides/frameworks' }] },
  ]),
];
