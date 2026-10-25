const modes = /** @type {const} */ (['observe', 'repair', 'strict']);
const operations = /** @type {const} */ (['read', 'write', 'unknown']);
const evidence = /** @type {const} */ (['clean', 'material', 'suppressed']);
const detectors = /** @type {const} */ (['Unit', 'Enumeration', 'Completeness', 'TypeConsistency', 'Charset']);
const phases = /** @type {const} */ (['both', 'response']);
export const matrix = modes.flatMap(mode => operations.flatMap(operation => evidence.flatMap(evidence => detectors.flatMap(detector =>
  phases.map(phase => ({ mode, operation, evidence, detector, phase, window: 128, payloadBytes: 862,
    id: `${mode}/${operation}/${evidence}/${detector}/${phase}/128/862` }))))));
for (const mode of modes) for (const operation of operations) for (const ev of evidence) {
  matrix.push({ mode, operation, evidence: ev, detector: 'Unit', phase: 'both', window: 16, payloadBytes: 862,
    id: `${mode}/${operation}/${ev}/Unit/both/16/862` });
  for (const phase of phases) matrix.push({ mode, operation, evidence: ev, detector: 'Unit', phase, window: 128, payloadBytes: 65536,
    id: `${mode}/${operation}/${ev}/Unit/${phase}/128/65536` });
}
/** @typedef {(typeof matrix)[number]} Case */
/** @param {Case} entry */
export function expectations(entry) {
  const eligible = entry.evidence === 'material' && entry.window === 128;
  const halted = eligible && entry.mode !== 'observe' && (entry.mode === 'strict' || entry.operation !== 'read');
  const requestHalt = halted && entry.phase === 'both';
  const numeric = entry.detector === 'Unit' || ((entry.detector === 'Completeness' || entry.detector === 'TypeConsistency') && entry.evidence !== 'material');
  const kinds = ['Enumeration', 'Completeness', 'TypeConsistency', 'Charset', ...(numeric ? ['Unit'] : [])].sort();
  const evaluatedPhases = entry.phase === 'response' || requestHalt ? 1 : 2;
  return { halted, requestHalt, kinds, detectors: kinds.length * evaluatedPhases * 6000,
    eligible: eligible ? entry.detector : undefined,
    suppressed: entry.evidence === 'suppressed' && entry.window >= 20 ? entry.detector : undefined,
    phases: evaluatedPhases };
}
/** @param {Case} entry */
export function fixture(entry) {
  const baselineValue = ['Enumeration', 'Charset'].includes(entry.detector) ? 'alpha' : 1;
  const changedValue = entry.detector === 'Unit' ? 100 : entry.detector === 'Completeness' ? null
    : entry.detector === 'TypeConsistency' ? '1' : entry.detector === 'Charset' ? '123' : 'bravo';
  const liveValue = entry.evidence === 'clean' ? baselineValue : changedValue;
  const baseline = Array.from({ length: entry.window }, () => JSON.stringify({ value: baselineValue }));
  const live = Array.from({ length: entry.window }, (_, i) => JSON.stringify({ value:
    entry.evidence === 'suppressed' && i % 4 === 0 ? baselineValue : liveValue }));
  const input = { value: liveValue, padding: '' };
  input.padding = 'x'.repeat(entry.payloadBytes - Buffer.byteLength(JSON.stringify(input)));
  if (Buffer.byteLength(JSON.stringify(input)) !== entry.payloadBytes) throw Error('fixture size changed');
  return { input, baseline, live, materiality: entry.evidence === 'suppressed' ? 1 : .2 };
}
