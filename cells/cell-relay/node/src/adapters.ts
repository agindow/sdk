import { warn } from './config.js';
import { AgindowSetupError } from './types.js';
import type { Callable, GuardOptions } from './types.js';
import type { InspectionHooks } from './guard.js';
import { RawJson, RAW_CAP } from './raw.js';
export type GuardFactory = <F extends Callable>(fn: F, options?: GuardOptions, hooks?: InspectionHooks) => F;
export interface ToolLike {
  readonly name?: string;
  readonly id?: string;
  readonly description?: string;
  readonly schema?: unknown;
  readonly func?: unknown;
  readonly execute?: unknown;
  readonly invoke?: unknown;
}
export type ToolMap = Readonly<Record<string, Callable | ToolLike>>;
export type ToolArray = readonly ToolLike[];
export interface ClientOptions extends Omit<GuardOptions, 'tool'> {
  readonly tool?: string | ((...args: never[]) => string);
}
export const HTTP_METHODS = ['get', 'post', 'put', 'patch', 'delete', 'request', 'fetch', 'proxy'] as const;
export const CLIENT_SKIPS = ['auth', 'connect', 'disconnect', 'batchSave', 'getCheckpoint', 'saveCheckpoint'] as const;
function own(value: object, key: PropertyKey): unknown { return Object.getOwnPropertyDescriptor(value, key)?.value; }
function immutableMethod(value: object, key: string): boolean {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  return descriptor?.configurable === false && descriptor.writable === false && typeof descriptor.value === 'function';
}
function toolOutput(output: unknown): unknown {
  if (output === null || typeof output !== 'object' || typeof own(output, 'tool_call_id') !== 'string') return output;
  const content = own(output, 'content');
  if (typeof content === 'string' && content.length <= RAW_CAP && Buffer.byteLength(content) <= RAW_CAP) {
    try {
      // Validate only the envelope syntax here. Rust inspects the original
      // tokens, so a validation parse never supplies rounded numeric evidence.
      const parsed: unknown = JSON.parse(content);
      if (parsed !== null && typeof parsed === 'object') return new RawJson(Buffer.from(content));
    } catch { /* ordinary tool text is not malformed vendor JSON */ }
  }
  return content ?? output;
}
function toolArguments(args: readonly unknown[], replacement: unknown): readonly unknown[] {
  const input = args[0];
  if (input !== null && typeof input === 'object' && own(input, 'type') === 'tool_call') {
    const prototype: unknown = Object.getPrototypeOf(input);
    if (prototype !== Object.prototype && prototype !== null) throw Error('opaque ToolCall repair unavailable');
    const descriptors = Object.getOwnPropertyDescriptors(input);
    const descriptor = descriptors['args'];
    if (descriptor === undefined || !('value' in descriptor)) throw Error('ToolCall args unavailable');
    descriptor.value = replacement;
    return [Object.defineProperties(Object.create(prototype), descriptors), ...args.slice(1)];
  }
  return [replacement, ...args.slice(1)];
}
function entry(value: unknown, key: string, make: GuardFactory, options: GuardOptions): unknown {
  if (typeof value === 'function') return make(value as Callable, { tool: key, ...options });
  if (value === null || typeof value !== 'object') return value;
  const prototype: unknown = Object.getPrototypeOf(value);
  // Generic class cloning loses private brands. Class integrations need their
  // executor hook; do not advertise a silently broken tool as guarded.
  if (prototype !== Object.prototype && prototype !== null) {
    const invoke = methods(value).get('invoke');
    const name = own(value, 'name');
    if (invoke !== undefined && typeof name === 'string' && own(value, 'schema') !== undefined) {
      if (immutableMethod(value, 'invoke')) {
        warn(`immutable executor ${key}.invoke; leaving tool unchanged`); return value;
      }
      const executor = make(function(...args: unknown[]): unknown { return Reflect.apply(invoke, value, args); },
        { tool: name, ...options }, {
          request(args) {
            const input = args[0];
            return input !== null && typeof input === 'object' && own(input, 'type') === 'tool_call'
              ? own(input, 'args') : input;
          },
          response(output) { return { inspect: true, value: toolOutput(output) }; },
          replaceRequest: toolArguments,
          replaceResponse() { throw Error('framework response repair needs a verified formatting boundary'); },
        });
      const bound = new Map<PropertyKey, unknown>();
      return new Proxy(value, { get(target, property) {
        if (property === 'invoke') return executor;
        const item: unknown = Reflect.get(target, property, target);
        if (typeof item !== 'function' || Object.hasOwn(target, property)) return item;
        if (!bound.has(property)) bound.set(property, item.bind(target));
        return bound.get(property);
      } });
    }
    warn(`tool ${key} requires a framework executor hook; leaving it unchanged`); return value;
  }
  const descriptors = Object.getOwnPropertyDescriptors(value);
  const executors = new Map<string, Callable>();
  const name: unknown = own(value, 'name') ?? own(value, 'id');
  const tool = typeof name === 'string' && name.length > 0 ? name : key;
  const operation = own(value, 'operation');
  for (const impl of ['func', 'execute', 'invoke']) {
    const descriptor = descriptors[impl];
    if (descriptor !== undefined && typeof descriptor.value === 'function') {
      if (descriptor.configurable === false && descriptor.writable === false) {
        warn(`immutable executor ${key}.${impl}; leaving tool unchanged`); return value;
      }
      const guarded = make(descriptor.value as Callable, { tool,
        ...(operation === 'read' || operation === 'write' || operation === 'unknown' ? { operation } : {}), ...options });
      const dispatcher = function(this: unknown, ...args: unknown[]): unknown {
        return Reflect.apply(guarded, this === proxy ? value : this, args);
      };
      Object.defineProperties(dispatcher, Object.getOwnPropertyDescriptors(guarded));
      executors.set(impl, dispatcher);
    }
  }
  if (executors.size === 0) { warn(`unrecognized tool ${key}; leaving it unchanged`); return value; }
  const proxy = new Proxy(value, { get(target, property) {
    return typeof property === 'string' && executors.has(property) ? executors.get(property) : Reflect.get(target, property, target);
  } });
  return proxy;
}
export function tools<T extends object>(target: T, make: GuardFactory, options: GuardOptions): T {
  try {
    const descriptors = Object.getOwnPropertyDescriptors(target);
    for (const key of Reflect.ownKeys(descriptors)) {
      if (key === 'length') continue;
      const descriptor: PropertyDescriptor | undefined = Reflect.get(descriptors, key);
      if (descriptor === undefined || !('value' in descriptor)) continue;
      try { descriptor.value = entry(descriptor.value, String(key), make, options); }
      catch { warn(`cannot wrap tool ${String(key)}; leaving it unchanged`); }
    }
    const output: object = Array.isArray(target) ? [] : Object.create(Object.getPrototypeOf(target));
    Object.defineProperties(output, descriptors); return output as T;
  } catch { warn('cannot install toolAdapter; leaving target unchanged'); return target; }
}
function methods(target: object): Map<string, Callable> {
  const result = new Map<string, Callable>();
  let current: object | null = target;
  const seen = new Set<string>();
  while (current !== null && current !== Object.prototype && current !== Function.prototype) {
    for (const key of Object.getOwnPropertyNames(current)) {
      if (seen.has(key)) continue; seen.add(key);
      const value: unknown = own(current, key);
      if (typeof value === 'function') result.set(key, value as Callable);
    }
    current = Object.getPrototypeOf(current) as object | null;
  }
  return result;
}
export function client<T extends object>(target: T, make: GuardFactory, options: ClientOptions): T {
  try {
    const { tool, ...config } = options;
    const wrapped = new Map<string, Callable>();
    const boundHelpers = new Map<PropertyKey, Callable>();
    const classClient = ![Object.prototype, null].includes(Object.getPrototypeOf(target) as object | null);
    for (const [key, fn] of methods(target)) {
      if (classClient && immutableMethod(target, key)) throw Error('immutable branded client method');
      if (!HTTP_METHODS.includes(key as typeof HTTP_METHODS[number])) continue;
      const bound = function(...args: unknown[]): unknown { return Reflect.apply(fn, target, args); };
      wrapped.set(key, make(bound, { ...config,
        ...(typeof tool === 'function' ? { toolName: tool } : { tool: tool ?? key }),
        operation: config.operation ?? (key === 'get' ? 'read' : 'unknown') }));
      const descriptor = Object.getOwnPropertyDescriptor(target, key);
      if (descriptor?.configurable === false && descriptor.writable === false) throw Error('immutable client method');
    }
    return new Proxy(target, { get(object, key) {
      if (typeof key === 'string' && wrapped.has(key)) return wrapped.get(key);
      const value: unknown = Reflect.get(object, key, object);
      // Branded class helpers need the original receiver too; they never enter
      // guard. Plain-object helpers retain exact function identity.
      if (classClient && typeof value === 'function') {
        let bound = boundHelpers.get(key);
        if (bound === undefined) { bound = value.bind(object) as Callable; boundHelpers.set(key, bound); }
        return bound;
      }
      return value;
    } });
  } catch { warn('cannot install clientAdapter; leaving target unchanged'); return target; }
}
/** Pure HTTP method maps are clients; mixed maps require explicit selection. */
export function detect(target: object): readonly ('toolAdapter' | 'clientAdapter' | 'httpAdapter' | 'guard')[] {
  if (typeof target === 'function') {
    if (Object.getOwnPropertyDescriptor(target, 'use') !== undefined) return ['httpAdapter'];
    if (target === globalThis.fetch || target.name === 'fetch') return ['httpAdapter'];
    return ['guard'];
  }
  if (Array.isArray(target)) return ['toolAdapter'];
  const keys = [...methods(target).keys()];
  const hasClient = keys.some(key => HTTP_METHODS.includes(key as typeof HTTP_METHODS[number]));
  const hasTool = !hasClient || keys.some(key => !HTTP_METHODS.includes(key as typeof HTTP_METHODS[number])
    && !CLIENT_SKIPS.includes(key as typeof CLIENT_SKIPS[number]) && key !== 'constructor');
  return [...(hasTool ? ['toolAdapter' as const] : []), ...(hasClient ? ['clientAdapter' as const] : [])];
}
export function ambiguity(candidates: readonly string[]): never {
  throw new AgindowSetupError(`Ambiguous wrap target: ${candidates.join(', ')}. Use ${candidates.map(name => `${name}(target, config)`).join(' or ')} explicitly.`);
}
