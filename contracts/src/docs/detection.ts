/** Shared homepage + docs detection quickstart; executed by the publication gate. */
import { detectionPython } from './detection-python.js';
import type { DocSnippet } from './model.js';
import { sdkMatrix } from './sdk-matrix.generated.js';

/** Stable fields printed by the sample; latencyUs and host timing vary per run. */
export const detectionExpectedFinding = {
  labelledChange: 'numeric magnitude shift',
  detector: 'Unit',
  event: 'drift',
  findings: 1,
  detectors: 5,
  eligibleDetectors: ['Unit'],
  class: 'semantic',
  bypassed: false,
  halt: false,
  repairApplied: false,
  resultUnchanged: true,
} as const;

export const detectionExpectedOutput = JSON.stringify(detectionExpectedFinding, null, 2);

export const detectionInstallCommand = `npm install agindow@${sdkMatrix.node.version}`;
export const detectionRunCommand = 'node --experimental-strip-types detect.mts';
export const detectionPrerequisites = [
  'Node.js 22 or newer',
  'A clean directory (no AGINDOW monorepo checkout required)',
  'Network access only to install the published npm package',
] as const;

/** Labelled simulated break: numeric magnitude shift (1 → 100). Shared by Node sample and `/try`. */
export function detectionDocuments(value: number): string[] {
  return Array.from({ length: 128 }, () => JSON.stringify({ value }));
}

/** Explicit statistical selection for the shared Unit demo (TII-107 / TII-109). */
export function detectionSelectionFor(live: number): {
  tenant: 'demo';
  connection: 'local';
  vendor: 'synthetic';
  operation: 'demo.detect';
  segment: 'default';
  fields: readonly [{
    side: 'request';
    path: '/value';
    baseline: string[];
    live: string[];
    enumeration: true;
  }];
} {
  return {
    tenant: 'demo',
    connection: 'local',
    vendor: 'synthetic',
    operation: 'demo.detect',
    segment: 'default',
    fields: [{
      side: 'request',
      path: '/value',
      baseline: detectionDocuments(1),
      live: detectionDocuments(live),
      enumeration: true,
    }],
  };
}

/**
 * Executable TypeScript sample: labelled Unit change under an explicit statistical
 * selection, plus unchanged / default / insufficient controls. Not a FakeEngine demo.
 */
export const detectionTs = `import { createAgindow } from 'agindow';
import type { Event, StatisticalSelection } from 'agindow';

/** Labelled simulated break: numeric magnitude shift (1 → 100). Not a vendor migration story. */
const documents = (value: number): string[] =>
  Array.from({ length: 128 }, () => JSON.stringify({ value }));

function selectionFor(live: number): StatisticalSelection {
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

async function observe(label: string, selection?: StatisticalSelection, bodyValue = 100): Promise<readonly Event[]> {
  const sdk = createAgindow(selection === undefined
    ? { mode: 'observe' }
    : { mode: 'observe', selection });
  await sdk.ready;
  const body = { value: bodyValue };
  const result = sdk.guard((input: { value: number }) => input, {
    tool: 'demo.detect',
    operation: 'read',
  })(body);
  if (result !== body) throw Error(\`\${label}: observe mutated the result (not an applied repair path)\`);
  if (sdk.recent().some((event: Event) => event.type === 'repair')) {
    throw Error(\`\${label}: unexpected repair event\`);
  }
  if (sdk.recent().some((event: Event) => event.type === 'bypass')) {
    throw Error(\`\${label}: unexpected bypass; this sample is detection, not an availability fixture\`);
  }
  return sdk.recent();
}

const changed = await observe('changed', selectionFor(100), 100);
const drift = changed.find((event: Event) => event.type === 'drift');
if (!drift?.verdict
  || drift.verdict.bypassed
  || drift.verdict.halt
  || drift.verdict.repair
  || drift.verdict.findings !== ${detectionExpectedFinding.findings}
  || drift.verdict.detectors !== ${detectionExpectedFinding.detectors}
  || drift.verdict.class !== '${detectionExpectedFinding.class}'
  || !drift.verdict.eligibleDetectors?.includes('${detectionExpectedFinding.detector}')) {
  throw Error('expected a real Unit finding for the labelled numeric magnitude shift');
}

const unchanged = await observe('unchanged', selectionFor(1), 1);
if (unchanged.some((event: Event) => event.type === 'drift')) {
  throw Error('unchanged live window reported drift');
}

const withoutDetector = await observe('default');
if (withoutDetector.some((event: Event) => event.type === 'drift')) {
  throw Error('default engine reported detection without an explicit selection');
}

const insufficient: StatisticalSelection = {
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
if (thin.some((event: Event) => event.type === 'drift')) {
  throw Error('insufficient windows reported a successful detection');
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
`;

export const detectionExample: DocSnippet = {
  samples: [{
    label: 'ts',
    language: 'ts',
    matrix: `node-local/${sdkMatrix.node.version}`,
    code: detectionTs,
  }, { label: 'py', language: 'python', matrix: `python-local/${sdkMatrix.python.version}`, code: detectionPython }],
};
