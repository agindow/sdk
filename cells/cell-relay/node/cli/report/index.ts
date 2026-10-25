import { readFileSync, lstatSync } from 'node:fs';
import { parseDriftEvent } from 'contracts';
import type { DriftEvent } from 'contracts';
import { scan } from './scan.js';
import type { ScanOptions } from './scan.js';
import { validateContract } from '../codegen.js';
import { simulateSandbox } from '../../src/sandbox.js';
import type { SandboxContract } from '../../src/sandbox.js';
export interface ReportOptions extends ScanOptions { readonly snapshot?: string }
interface Recording { readonly tool: string; readonly contract: SandboxContract; readonly args: unknown; readonly recorded: unknown }
function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function snapshot(path: string): { events: DriftEvent[]; start: string; end: string; recordings: Recording[] } {
  if (lstatSync(path).size > 1024 * 1024) throw Error('snapshot too large');
  const input: unknown = JSON.parse(readFileSync(path, 'utf8'));
  if (!record(input) || input['version'] !== 1 || !record(input['window']) || !Array.isArray(input['events']) || input['events'].length > 10_000) throw Error('invalid snapshot');
  const window = input['window']; const start = window['start']; const end = window['end'];
  if (typeof start !== 'string' || typeof end !== 'string' || !Number.isFinite(Date.parse(start)) || !Number.isFinite(Date.parse(end)) || Date.parse(start) > Date.parse(end)) throw Error('invalid window');
  const events = input['events'].map((event: unknown) => parseDriftEvent(event));
  if (new Set(events.map(event => event.id)).size !== events.length) throw Error('duplicate event IDs');
  if (events.some(event => Date.parse(event.ts) < Date.parse(start) || Date.parse(event.ts) > Date.parse(end))) throw Error('event outside window');
  const recordings: Recording[] = [];
  if (input['recordings'] !== undefined) {
    if (!Array.isArray(input['recordings']) || input['recordings'].length > 256) throw Error('invalid recordings');
    for (const recording of input['recordings']) {
      if (!record(recording) || typeof recording['tool'] !== 'string' || !Object.hasOwn(recording, 'recorded')) throw Error('invalid recording');
      const contract = recording['contract']; validateContract(contract);
      if (contract.tool !== recording['tool']) throw Error('recording tool mismatch');
      recordings.push({ tool: recording['tool'], contract, args: recording['args'], recorded: recording['recorded'] });
    }
  }
  return { events, start, end, recordings };
}
/** Static exposure analysis. Recorded events are evidence, never runtime repair
 * authorization. Unknown coverage is explicitly unavailable. No source executes. */
export function report(directory: string, options: ReportOptions = {}) {
  const result = scan(directory, options);
  const vendors = [...new Set(result.tools.flatMap(tool => tool.vendors))].sort();
  const lines = [`Tools detected: ${result.tools.length}`, `Already wrapped: ${result.tools.filter(tool => tool.wrapped).length}`,
    `Vendors: ${vendors.length} (${vendors.join(', ')})`, `Skipped files: ${result.skipped}`];
  let events: DriftEvent[] = [];
  let recordings: Recording[] = [];
  if (options.snapshot === undefined) lines.push('No registry snapshot; drift evidence unavailable.');
  else try {
    const known = snapshot(options.snapshot); events = known.events; recordings = known.recordings;
    lines.push(`Known drift events: ${events.length} (${known.start} – ${known.end})`);
  } catch { lines.push('Invalid registry snapshot; drift evidence unavailable.'); }
  for (const tool of result.tools) {
    const matching = events.filter(event => event.tool === tool.name);
    const recorded = recordings.filter(item => item.tool === tool.name);
    if (matching.length === 0 && recorded.length === 0) { lines.push(`  ${tool.name} → evidence unavailable`); continue; }
    for (const event of matching) {
      const change = (event.drift.changes ?? []).map(item => `${item.kind}: ${item.path}${item.to === undefined ? '' : ` → ${item.to}`}`).join('; ');
      const verdict = event.drift.class === 'semantic' ? 'BLOCK-class warning (semantic)' : event.drift.class === 'shape'
        ? 'shape repair candidate (execution unverified)' : 'unknown drift; review required';
      lines.push(`  ${tool.name} → ${verdict}: ${change}; confidence ${event.confidence ?? 'unavailable'}`);
    }
    for (const recording of recorded) {
      const verdict = simulateSandbox(recording.contract, recording.args, recording.recorded);
      const description = verdict.driftClass === 'semantic' ? 'BLOCK-class warning (semantic; never an automatic conversion)'
        : verdict.status === 'repaired' ? `would repair ${verdict.changes.length} change(s), confidence ${verdict.confidence}`
        : verdict.status === 'ok' ? 'no drift observed' : verdict.status;
      lines.push(`  ${tool.name} → ${description} (recorded local sandbox fixture)`);
    }
  }
  return { ...result, vendors, text: lines.join('\n') };
}
