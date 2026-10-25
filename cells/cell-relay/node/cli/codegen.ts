import { createHash } from 'node:crypto';
import { readFileSync, lstatSync, realpathSync, writeFileSync, renameSync, unlinkSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import runtime from '../src/sandbox.ts?raw';
import type { SandboxContract } from '../src/sandbox.js';
import { parse, visit, syntax, name, literal, property, member } from './report/ast.js';
function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function fields(value: unknown, request: boolean): void {
  if (!record(value) || !record(value['fields']) || Object.keys(value['fields']).length > 64) throw Error('invalid fields');
  for (const [key, rule] of Object.entries(value['fields'])) {
    if (key.length === 0 || key.length > 128 || ['__proto__', 'constructor', 'prototype'].includes(key) || !record(rule)
      || !['string', 'number', 'boolean', 'null'].includes(String(rule['type']))
      || (rule['required'] !== undefined && typeof rule['required'] !== 'boolean')
      || (rule['enum'] !== undefined && (!Array.isArray(rule['enum']) || rule['enum'].length > 64 || rule['enum'].some(item => typeof item !== 'string')))) throw Error('invalid field rule');
  }
  const renames = value['renames'];
  if (renames !== undefined) {
    if (!request || !Array.isArray(renames) || renames.length > 32) throw Error('invalid rename rules');
    const used = new Set<string>();
    for (const rule of renames) {
      if (!record(rule) || typeof rule['from'] !== 'string' || typeof rule['to'] !== 'string' || rule['from'] === rule['to']
        || typeof rule['identity'] !== 'string' || rule['identity'].length === 0 || typeof rule['confidence'] !== 'number'
        || !Number.isFinite(rule['confidence']) || rule['confidence'] < 0 || rule['confidence'] > 1
        || !Object.hasOwn(value['fields'], rule['to']) || [rule['from'], rule['to']].some(key => ['__proto__', 'constructor', 'prototype'].includes(key) || key.length > 128 || used.has(key))) throw Error('invalid confirmed sibling rename');
      used.add(rule['from']); used.add(rule['to']);
    }
  }
}
export function validateContract(value: unknown): asserts value is SandboxContract {
  if (!record(value) || value['version'] !== 1 || Buffer.byteLength(JSON.stringify(value)) > 2048
    || !['get', 'post', 'put', 'patch', 'delete', 'request'].includes(String(value['method']))
    || !['observe', 'repair', 'strict'].includes(String(value['mode'])) || !['read', 'write', 'unknown'].includes(String(value['operation']))) throw Error('invalid sandbox contract');
  for (const key of ['id', 'file', 'tool', 'endpoint']) if (typeof value[key] !== 'string' || value[key].length === 0) throw Error(`missing ${key}`);
  if (value['disabled'] !== undefined && typeof value['disabled'] !== 'boolean') throw Error('invalid opt-out');
  const file = String(value['file']);
  if (isAbsolute(file) || file.split(/[\\/]/).includes('..') || !/(?:^|\/)(?:actions|syncs)\/[^/]+\.ts$/.test(file)) throw Error('expected action/sync file');
  if (value['request'] !== undefined) fields(value['request'], true);
  if (value['response'] !== undefined) fields(value['response'], false);
}
function symbol(contract: SandboxContract): string { return `__agw_${createHash('sha256').update(contract.file).digest('hex').slice(0, 12)}`; }
const start = '/* AGINDOW GUARD v1 START */'; const end = '/* AGINDOW GUARD v1 END */';
export function generateGuard(contract: SandboxContract): string {
  validateContract(contract);
  const source = runtime.replace(/^export /gm, '');
  return `${start}\nconst ${symbol(contract)} = (() => {\n${source}\nconst contract: SandboxContract = ${JSON.stringify(contract)};\nreturn async function<P, R>(context: object, call: (request: P) => Promise<R>, request: P): Promise<R> {\n const input: unknown = request !== null && typeof request === 'object' ? Object.getOwnPropertyDescriptor(request, 'data')?.value : undefined;\n const before = evaluateSandbox(contract, input, 'request');\n const halt = (result: SandboxResult): never => { const ActionError: unknown = Reflect.get(context, 'ActionError'); if (typeof ActionError !== 'function') throw Error('Sandbox ActionError unavailable'); throw Reflect.construct(ActionError, [{ error: contract.mode === 'strict' ? 'drift_halted' : 'drift_blocked', tool: contract.tool, drift_class: result.driftClass, drift_id: contract.id, hint: result.changes.map(change => change.path + ': ' + change.kind + (change.to === undefined ? '' : ' → ' + change.to)).join('; ') + (result === before ? '. Request was not dispatched.' : '. Upstream completed; do not blindly replay a write.'), audit: '' }]); };\n if (before.status === 'blocked') halt(before);\n if (before.status === 'repaired' && request !== null && typeof request === 'object') { try { request = Object.create(Object.getPrototypeOf(request), { ...Object.getOwnPropertyDescriptors(request), data: { value: before.value, configurable: true, enumerable: true, writable: true } }) as P; } catch { return await call(request); } }\n try { if (before.status !== 'ok') console.warn('AGINDOW: ' + before.status + '; byte_identical=' + before.byte_identical + '; local sandbox gate, no write-integrity proof.'); } catch {}\n const response = await call(request);\n const output: unknown = response !== null && typeof response === 'object' ? Object.getOwnPropertyDescriptor(response, 'data')?.value : response;\n if (before.status === 'bypassed') return response;\n const after = evaluateSandbox(contract, output, 'response');\n try { if (after.status !== 'ok') console.warn('AGINDOW: ' + after.status + '; local sandbox gate.'); } catch {}\n if (after.status === 'blocked') halt(after);\n return response;\n};\n})();\n${end}\n`;
}
export function instrument(source: string, contract: SandboxContract): string {
  validateContract(contract); const block = generateGuard(contract); const tree = parse(source);
  const position = source.indexOf(start); const finish = source.indexOf(end);
  if (position >= 0 || finish >= 0) {
    if (position < 0 || finish < position) throw Error('invalid generated markers');
    if (source.indexOf(start, position + start.length) >= 0 || source.indexOf(end, finish + end.length) >= 0) throw Error('duplicate generated markers');
    // Generated markers must delimit a real top-level declaration and cannot
    // be text inside a string or a partially edited block.
    const body = tree['body'];
    const declaration = Array.isArray(body) ? body.find(item => syntax(item) && item.type === 'VariableDeclaration' && item.start > position && item.end < finish) : undefined;
    if (!syntax(declaration) || !source.slice(declaration.start, declaration.end).startsWith(`const ${symbol(contract)} =`)) throw Error('marker collision');
    let bindings = 0;
    visit(tree, node => {
      if (node.type !== 'CallExpression' || name(node['callee']) !== symbol(contract)) return;
      bindings++;
      const args = node['arguments'];
      if (!Array.isArray(args) || args.length !== 3 || !syntax(args[1]) || !syntax(args[2])) throw Error('invalid generated call binding');
      const callback = args[1]; const request = args[2];
      const body = callback['body'];
      if (callback.type !== 'ArrowFunctionExpression' || !syntax(body) || body.type !== 'CallExpression'
        || literal(property(request, 'endpoint')) !== contract.endpoint) throw Error('generated endpoint binding changed');
      const path = member(body['callee']);
      if (path.length !== 2 || path[0] !== name(args[0]) || path[1] !== contract.method) throw Error('generated method binding changed');
    });
    if (bindings === 0) throw Error('generated provider binding missing');
    return source.slice(0, position) + block + source.slice(finish + end.length).replace(/^\r?\n/, '');
  }
  const builders = new Set<string>();
  visit(tree, node => {
    if (node.type !== 'ImportDeclaration' || !syntax(node['source']) || node['source']['value'] !== 'nango' || !Array.isArray(node['specifiers'])) return;
    for (const specifier of node['specifiers']) if (syntax(specifier) && ['createAction', 'createSync'].includes(name(specifier['imported']) ?? '')) {
      const local = name(specifier['local']); if (local !== undefined) builders.add(local);
    }
  });
  const edits: { start: number; end: number; value: string }[] = [];
  visit(tree, (node, ancestors) => {
    if (node.type !== 'CallExpression' || !Array.isArray(node['arguments']) || node['arguments'].length !== 1) return;
    const path = member(node['callee']); if (path.length !== 2 || path[1] !== contract.method) return;
    const request: unknown = node['arguments'][0];
    if (!syntax(request) || request.type !== 'ObjectExpression' || literal(property(request, 'endpoint')) !== contract.endpoint) return;
    const exec = ancestors.find(ancestor => ancestor.type === 'Property' && name(ancestor['key']) === 'exec');
    if (exec === undefined || !syntax(exec['value']) || !Array.isArray(exec['value']['params']) || name(exec['value']['params'][0]) !== path[0]) return;
    const builder = ancestors.find(ancestor => ancestor.type === 'CallExpression' && builders.has(name(ancestor['callee']) ?? ''));
    if (builder === undefined) return;
    const call = source.slice(node.start, request.start) + 'request' + source.slice(request.end, node.end);
    edits.push({ start: node.start, end: node.end, value: `${symbol(contract)}(${path[0]}, (request) => ${call}, ${source.slice(request.start, request.end)})` });
  });
  if (edits.length === 0 || source.includes(symbol(contract))) throw Error('no supported provider call, or generated identifier collision');
  const ordered = [...edits].sort((a, b) => a.start - b.start);
  for (let index = 1; index < ordered.length; index++) {
    const prior = ordered[index - 1]; const next = ordered[index];
    if (prior !== undefined && next !== undefined && prior.end > next.start) throw Error('nested provider calls are unsupported');
  }
  for (const edit of edits.sort((a, b) => b.start - a.start)) source = source.slice(0, edit.start) + edit.value + source.slice(edit.end);
  parse(block + source); return block + source;
}
export function readContracts(path: string, disabled = false): SandboxContract[] {
  if (lstatSync(path).size > 1024 * 1024) throw Error('snapshot too large');
  const value: unknown = JSON.parse(readFileSync(path, 'utf8'));
  if (!record(value) || value['version'] !== 1 || !Array.isArray(value['contracts']) || value['contracts'].length > 256) throw Error('invalid generation input');
  const seen = new Set<string>(); const contracts: SandboxContract[] = [];
  for (const entry of value['contracts']) {
    validateContract(entry); if (seen.has(entry.file)) throw Error('one contract per file is supported'); seen.add(entry.file);
    contracts.push({ ...entry, ...(disabled ? { disabled: true } : {}) });
  }
  return contracts;
}
export function patchProject(directory: string, contracts: readonly SandboxContract[]): { changed: number; skipped: number } {
  const root = realpathSync(directory); let changed = 0; let skipped = 0;
  for (const contract of contracts) {
    let temporary: string | undefined;
    try {
      validateContract(contract); const path = resolve(root, contract.file);
      const real = realpathSync(path); const local = relative(root, real);
      if (local.startsWith(`..${sep}`) || isAbsolute(local) || real !== path || lstatSync(path).size > 1024 * 1024) throw Error('unsafe or oversized path');
      const original = readFileSync(path, 'utf8'); const output = instrument(original, contract);
      if (original === output) continue;
      temporary = resolve(dirname(path), `.${symbol(contract)}-${process.pid}.tmp`);
      writeFileSync(temporary, output, { flag: 'wx', mode: lstatSync(path).mode });
      if (readFileSync(path, 'utf8') !== original) throw Error('source changed during generation');
      renameSync(temporary, path); temporary = undefined; changed++;
    } catch { skipped++; }
    finally { if (temporary !== undefined) try { unlinkSync(temporary); } catch { /* best effort */ } }
  }
  return { changed, skipped };
}
