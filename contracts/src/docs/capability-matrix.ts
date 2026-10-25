/** Reviewed pre-alpha evaluation matrix (TII-108). Provenance: relay docs, package metadata, paired SDK matrix. */
import { sdkMatrix } from './sdk-matrix.generated.js';

export type CapabilityEvidence = {
  readonly label: string;
  readonly href: string;
};

export type CapabilityId = 'detection' | 'sdks' | 'repair' | 'production' | 'evidence';

export type CapabilityRow = {
  readonly id: CapabilityId;
  readonly capability: string;
  readonly evaluateToday: string;
  readonly scope: string;
  readonly configuration: string;
  readonly limitation: string;
  readonly evidence: CapabilityEvidence;
};

/** Named statistical change families covered by current implementation tests (statistical_audit). */
export const capabilityChangeFamilies = [
  'magnitude (Unit)',
  'enumeration',
  'completeness',
  'type',
  'charset / format',
] as const;

export const capabilityMatrixColumns = [
  'Capability',
  'Evaluate today',
  'Scope',
  'Configuration',
  'Limitation',
  'Evidence',
] as const;

/** Declared package floors versus tested runtimes — do not infer untested upper bounds. */
export const capabilityMatrixRuntimes = {
  node: {
    engines: '>=22',
    tested: 'Node v22.21.1',
    source: 'package.json engines + local PERFORMANCE.md and detection walkthrough',
  },
  python: {
    requires: '>=3.11',
    tested: 'Python 3.12 (relay CI)',
    source: 'pyproject requires-python + relay workflow python-version',
  },
} as const;

const families = capabilityChangeFamilies.join(', ');

export const capabilityMatrix = {
  caption: 'Pre-alpha capabilities you can evaluate today',
  node: capabilityMatrixRuntimes.node,
  python: capabilityMatrixRuntimes.python,
  rows: [
    {
      id: 'detection',
      capability: 'Local drift detection',
      evaluateToday: `Explicit statistical detection for ${families}. Public /try runs a labelled Unit finding in the browser against the same packaged WASM guest; /docs/quickstart remains the install path.`,
      scope: 'Embedded WASM guest in published Node/Python SDKs. Native library seams exist separately and are not the default guest or every SDK adapter.',
      configuration: 'Select an explicit detector with scoped baseline/live windows, await readiness, then wrap calls. Mode (observe/repair/strict) does not configure a detector.',
      limitation: 'The default engine passes calls through without detecting drift until a detector is selected and configured. Detection against production traffic has not been calibrated.',
      evidence: { label: 'Playground', href: '/try' },
    },
    {
      id: 'sdks',
      capability: 'Node / Python SDKs',
      evaluateToday: `Install published pre-alpha packages agindow@${sdkMatrix.node.version} (npm) and agindow==${sdkMatrix.python.version} (PyPI). Adapters, CLI report and parity fixtures exercise the shared WASM bytes.`,
      scope: `Declared package metadata: Node ${capabilityMatrixRuntimes.node.engines}; Python ${capabilityMatrixRuntimes.python.requires}. Tested at ${capabilityMatrixRuntimes.node.tested} and ${capabilityMatrixRuntimes.python.tested}; untested upper bounds are not claimed.`,
      configuration: 'Registry install only. Contributors changing the engine still build the shared WASM guest. No hosted key required for embedded use.',
      limitation: 'Publication is not protection. Registry install does not enable detection, repair or hosted delivery.',
      evidence: { label: 'SDK docs', href: '/docs/sdk' },
    },
    {
      id: 'repair',
      capability: 'Repair proposals / verified repair',
      evaluateToday: 'The opt-in native call path can propose selective patches and apply only independently verified repairs for mechanical renames confirmed by a trusted migration source. Synthetic internal repair-safety cases gate regressions.',
      scope: 'Opt-in native call path and repair library. The default WASM guest does not supply selective repair output.',
      configuration: 'Trusted engine integration, sufficient confidence, independent verification gates and repair/strict mode. Adapter install or mode alone is insufficient.',
      limitation: 'Proposed ≠ applied. Schema acceptance and confidence alone cannot establish intent. Ambiguous semantic mappings are not safe migrations. Default guest has no selective repair.',
      evidence: {
        label: 'Modes and repair limits',
        href: '/docs/modes',
      },
    },
    {
      id: 'production',
      capability: 'Production traffic use',
      evaluateToday: 'Local policy matrix, availability bypass, audit composition and write-protection seams are implemented and testable in-repo.',
      scope: 'Local library and Worker builds only. Sidecar and hosted HTTP distributions exist as explicit scripts; neither is deployed or validated against live production traffic.',
      configuration: 'Your deployment must provide the detector, verification callbacks and controls for turning protection on. Nothing turns itself on for production traffic.',
      limitation: 'AGINDOW does not currently operate a hosted write-protection service, and calibrated detection for production traffic remains unavailable. The homepage capability matrix records the remaining work without dates or unsupported promises.',
      evidence: {
        label: 'Capability matrix',
        href: '/#maturity',
      },
    },
    {
      id: 'evidence',
      capability: 'Tests / benchmarks',
      evaluateToday: 'Reproducible local commands: cargo/vitest statistical and gated suites, internal repair-safety eval (42 synthetic cases), and local native/WASM performance fixtures.',
      scope: 'Synthetic and local evidence only. Nothing has run against production traffic.',
      configuration: 'Follow published docs for install/quickstart baselines. Set hardware/runtime context when interpreting latency fixtures.',
      limitation: 'Internal repair-safety results are not detector accuracy. Latency numbers in docs are targets or local fixture means — not measured production latency.',
      evidence: {
        label: 'Quickstart',
        href: '/docs/quickstart',
      },
    },
  ] as const satisfies readonly CapabilityRow[],
} as const;
