/**
 * Local detection quickstart (TII-107). Same behaviour as the reviewed docs sample
 * in contracts/src/docs/detection.ts — run after `npm install` in this directory.
 */
import { createAgindow } from 'agindow';

/** Labelled simulated break: numeric magnitude shift (1 → 100). Not a vendor migration story. */
const documents = (value) => Array.from({ length: 128 }, () => JSON.stringify({ value }));

function selectionFor(live) {
  return {
    tenant: 'demo',
    connection: 'local',
    vendor: 'synthetic',
    operation: 'demo.detect',
    segment: 'default',
    fields: [{
      side: 'request',
      path: '/value',
      baseline: documents(1),
      live: documents(live),
      enumeration: true,
    }],
  };
}

async function observe(label, selection, bodyValue = 100) {
  const sdk = createAgindow(selection === undefined
    ? { mode: 'observe' }
    : { mode: 'observe', selection });
  await sdk.ready;
  const body = { value: bodyValue };
  const result = sdk.guard((input) => input, {
    tool: 'demo.detect',
    operation: 'read',
  })(body);
  if (result !== body) throw new Error(`${label}: observe mutated the result (not an applied repair path)`);
  if (sdk.recent().some((event) => event.type === 'repair')) {
    throw new Error(`${label}: unexpected repair event`);
  }
  if (sdk.recent().some((event) => event.type === 'bypass')) {
    throw new Error(`${label}: unexpected bypass; this sample is detection, not an availability fixture`);
  }
  return sdk.recent();
}

const changed = await observe('changed', selectionFor(100), 100);
const drift = changed.find((event) => event.type === 'drift');
if (!drift?.verdict
  || drift.verdict.bypassed
  || drift.verdict.halt
  || drift.verdict.repair
  || drift.verdict.findings !== 1
  || drift.verdict.detectors !== 5
  || drift.verdict.class !== 'semantic'
  || !drift.verdict.eligibleDetectors?.includes('Unit')) {
  throw new Error('expected a real Unit finding for the labelled numeric magnitude shift');
}

const unchanged = await observe('unchanged', selectionFor(1), 1);
if (unchanged.some((event) => event.type === 'drift')) {
  throw new Error('unchanged live window reported drift');
}

const withoutDetector = await observe('default');
if (withoutDetector.some((event) => event.type === 'drift')) {
  throw new Error('default engine reported detection without an explicit selection');
}

const insufficient = {
  tenant: 'demo',
  connection: 'local',
  vendor: 'synthetic',
  operation: 'demo.detect',
  segment: 'default',
  fields: [{
    side: 'request',
    path: '/value',
    baseline: Array.from({ length: 4 }, () => '{"value":1}'),
    live: Array.from({ length: 4 }, () => '{"value":100}'),
    enumeration: true,
  }],
};
const thin = await observe('insufficient', insufficient, 100);
if (thin.some((event) => event.type === 'drift')) {
  throw new Error('insufficient windows reported a successful detection');
}

const observed = {
  labelledChange: 'numeric magnitude shift',
  detector: drift.verdict.eligibleDetectors.find(name => name === 'Unit'),
  event: drift.type,
  findings: drift.verdict.findings,
  detectors: drift.verdict.detectors,
  eligibleDetectors: drift.verdict.eligibleDetectors,
  class: drift.verdict.class,
  bypassed: Boolean(drift.verdict.bypassed),
  halt: Boolean(drift.verdict.halt),
  repairApplied: Boolean(drift.verdict.repair),
  resultUnchanged: true, // observe() checked reference identity above.
};
console.log(JSON.stringify(observed, null, 2));
console.log('Variable per run (not part of the stable finding above): latencyUs and host timing.');
