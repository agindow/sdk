import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { AgindowSetupError } from './types.js';
import type { Config } from './types.js';

export function warn(message: string): void {
  try { console.warn(`AGINDOW: ${message}`); } catch { /* diagnostic sinks are untrusted */ }
}
function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function strings(value: unknown): boolean {
  return Array.isArray(value) && value.every(item => typeof item === 'string');
}
/** Validate each supplied source before merging, including shadowed sources. */
export function validate(value: unknown): asserts value is Config {
  if (!record(value)) throw new AgindowSetupError('config must be an object');
  const fail = (key: string): never => { throw new AgindowSetupError(`invalid config: ${key}`); };
  if (value['mode'] !== undefined && !['observe', 'repair', 'strict'].includes(String(value['mode']))) fail('mode');
  for (const key of ['failClosed', 'spans', 'debug']) {
    if (value[key] !== undefined && typeof value[key] !== 'boolean') fail(key);
  }
  if (value['buffer'] !== undefined && (!Number.isSafeInteger(value['buffer']) || Number(value['buffer']) < 0
    || Number(value['buffer']) > 100_000)) fail('buffer (0–100000)');
  if (value['onEvent'] !== undefined && typeof value['onEvent'] !== 'function') fail('onEvent');
  if (value['apiKey'] !== undefined && typeof value['apiKey'] !== 'string') fail('apiKey');
  for (const key of ['endpoint', 'sidecar']) {
    if (value[key] !== undefined) {
      try { if (typeof value[key] !== 'string' || !['http:', 'https:'].includes(new URL(value[key]).protocol)) fail(key); }
      catch { fail(key); }
    }
  }
  if (value['allowedTools'] !== undefined && value['allowedTools'] !== '*' && !strings(value['allowedTools'])) fail('allowedTools');
  if (value['denyTools'] !== undefined && !strings(value['denyTools'])) fail('denyTools');
  if (value['sync'] !== undefined) {
    if (!record(value['sync'])) fail('sync');
    else for (const [key, interval] of Object.entries(value['sync'])) {
      if (!['contracts', 'drift'].includes(key) || typeof interval !== 'number' || !Number.isFinite(interval) || interval <= 0) fail('sync');
    }
  }
  for (const key of ['headers', 'transformers']) {
    const entries = value[key];
    if (entries !== undefined && (!record(entries) || Object.values(entries).some(entry =>
      typeof entry !== (key === 'headers' ? 'string' : 'function')))) fail(key);
  }
}
function localConfig(cwd: string): Config {
  let directory = resolve(cwd);
  for (;;) {
    try {
      const value: unknown = JSON.parse(readFileSync(join(directory, '.agindowrc'), 'utf8'));
      validate(value); return value;
    } catch (error) {
      if (!(record(error) && error['code'] === 'ENOENT')) {
        warn(`cannot read valid .agindowrc in ${directory}; ignoring it`); return {};
      }
    }
    const parent = dirname(directory);
    if (parent === directory) return {};
    directory = parent;
  }
}
export function resolveConfig(explicit: Config = {}, sources: {
  readonly cwd?: string; readonly env?: Readonly<Record<string, string | undefined>>;
} = {}): Config {
  const env = sources.env ?? process.env;
  const environment: Record<string, unknown> = {};
  for (const [key, name] of [['mode', 'AGINDOW_MODE'], ['apiKey', 'AGINDOW_KEY'], ['endpoint', 'AGINDOW_ENDPOINT'], ['sidecar', 'AGINDOW_SIDECAR']] as const) {
    if (env[name] !== undefined) environment[key] = env[name];
  }
  if (env['AGINDOW_BUFFER'] !== undefined) environment['buffer'] = Number(env['AGINDOW_BUFFER']);
  for (const [key, name] of [['spans', 'AGINDOW_SPANS'], ['failClosed', 'AGINDOW_FAIL_CLOSED'], ['debug', 'AGINDOW_DEBUG']] as const) {
    if (env[name] !== undefined) {
      if (!['true', 'false'].includes(env[name])) throw new AgindowSetupError(`invalid ${name}`);
      environment[key] = env[name] === 'true';
    }
  }
  validate(environment); validate(explicit);
  const config = { mode: 'observe' as const, failClosed: true, buffer: 100, spans: true, allowedTools: '*' as const,
    ...localConfig(sources.cwd ?? process.cwd()), ...environment, ...explicit };
  if (config.failClosed === false) warn('fail-open writes requested. Semantic writes remain protected in repair mode; strict mode ignores this opt-out.');
  return config;
}
