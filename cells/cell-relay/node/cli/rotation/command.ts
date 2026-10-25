import { jsonLines } from './input.js';
import { rotationReport, table } from './index.js';
export async function rotationCommand(args: readonly string[]): Promise<number> {
  const [events, factsFile, ...flags] = args;
  if (events === undefined || factsFile === undefined) throw new Error('Rotation input required');
  let now: string | undefined;
  for (let i = 0; i < flags.length; i++) {
    const f = flags[i];
    if (f === '--as-of') { now = flags[++i]; if (now === undefined) throw new Error('Missing as-of'); }
    else if (f !== '--json' && f !== '--require=verifiable') throw new Error('Unknown rotation flag');
  }
  if (now === undefined) throw new Error('Deterministic --as-of UTC timestamp required');
  const facts: unknown[] = [];
  for await (const fact of jsonLines(factsFile)) { if (facts.length >= 65536) throw new Error('Too many capability facts'); facts.push(fact); }
  const report = await rotationReport(jsonLines(events), facts, { now, staleMs: 30 * 86400000, connectionCap: 4096, operationCap: 256 });
  console.log(flags.includes('--json') ? JSON.stringify(report) : table(report.rows));
  return flags.includes('--require=verifiable') && (report.rows.length === 0 || report.rows.some(row => row.verdict !== 'verifiable')) ? 1 : 0;
}
