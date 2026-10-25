import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import type { Span, Tracer } from '@opentelemetry/api';
import type { Event, Mode } from './types.js';
declare const __AGINDOW_FILENAME__: string;
let telemetry: typeof import('@opentelemetry/api') | null | undefined;
/** Optional peer: an offline install needs no telemetry package or exporter. */
function tracer(): Tracer | undefined {
  try {
    if (telemetry === undefined) {
      const filename = typeof __AGINDOW_FILENAME__ === 'undefined' ? resolve(process.cwd(), 'package.json') : __AGINDOW_FILENAME__;
      try { telemetry = createRequire(filename)('@opentelemetry/api') as typeof import('@opentelemetry/api'); }
      catch { telemetry = null; }
    }
    return telemetry?.trace.getTracer('agindow');
  } catch { return undefined; }
}
// Keep fileURLToPath available for the ESM build's filename substitution.
void fileURLToPath;
export function openSpan(enabled: boolean, tool: string, mode: Mode): {
  event(event: Event): void; end(failed?: boolean): void;
} {
  let span: Span | undefined;
  try { if (enabled) span = tracer()?.startSpan(`agindow ${tool}`, {
    attributes: { 'agindow.tool': tool, 'agindow.mode': mode, 'agindow.status': 'ok' },
  }); } catch { /* SDK faults cannot become agent errors */ }
  return {
    event(event) {
      try {
        span?.setAttribute('agindow.status', event.type === 'bypass' ? 'bypassed' : event.type);
        if (event.driftClass !== undefined) span?.setAttribute('agindow.drift.class', event.driftClass);
        if (event.auditId !== undefined) span?.setAttribute('agindow.audit.id', event.auditId);
        if (event.confidence !== undefined) span?.setAttribute('agindow.confidence', event.confidence);
      } catch { /* instrumentation only */ }
    },
    end(failed = false) {
      try { if (failed) span?.setAttribute('agindow.upstream.error', true); } catch { /* instrumentation only */ }
      try { span?.end(); } catch { /* instrumentation only */ }
    },
  };
}
