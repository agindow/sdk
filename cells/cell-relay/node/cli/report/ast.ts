import { Parser } from 'acorn';
import { tsPlugin } from 'acorn-typescript';
/** The parser boundary is erased once; callers inspect only static syntax. */
export interface Syntax {
  readonly type: string;
  readonly start: number;
  readonly end: number;
  readonly [key: string]: unknown;
}
const parser = Parser.extend(tsPlugin() as unknown as (base: typeof Parser) => typeof Parser);
export function syntax(value: unknown): value is Syntax {
  return value !== null && typeof value === 'object' && 'type' in value && typeof value.type === 'string'
    && 'start' in value && typeof value.start === 'number' && 'end' in value && typeof value.end === 'number';
}
export function parse(source: string): Syntax {
  return parser.parse(source, { ecmaVersion: 'latest', sourceType: 'module', locations: true }) as unknown as Syntax;
}
export function children(node: Syntax): Syntax[] {
  const result: Syntax[] = [];
  for (const [key, value] of Object.entries(node)) {
    if (key === 'loc') continue;
    if (syntax(value)) result.push(value);
    else if (Array.isArray(value)) for (const child of value) if (syntax(child)) result.push(child);
  }
  return result;
}
export function visit(node: Syntax, callback: (node: Syntax, ancestors: readonly Syntax[]) => void, ancestors: readonly Syntax[] = []): void {
  if (ancestors.length > 128) throw Error('syntax depth limit');
  callback(node, ancestors);
  for (const child of children(node)) visit(child, callback, [...ancestors, node]);
}
export function name(value: unknown): string | undefined {
  if (!syntax(value)) return undefined;
  if (value.type === 'Identifier' && typeof value['name'] === 'string') return value['name'];
  if (value.type === 'Literal' && typeof value['value'] === 'string') return value['value'];
  return undefined;
}
export function literal(value: unknown): string | undefined {
  return syntax(value) && value.type === 'Literal' && typeof value['value'] === 'string' ? value['value'] : undefined;
}
export function property(node: Syntax, key: string): Syntax | undefined {
  const props = node['properties'];
  if (!Array.isArray(props)) return undefined;
  for (const prop of props) if (syntax(prop) && prop.type === 'Property' && prop['computed'] !== true
    && name(prop['key']) === key && syntax(prop['value'])) return prop['value'];
  return undefined;
}
export function member(node: unknown): string[] {
  if (!syntax(node)) return [];
  if (node.type === 'Identifier') return name(node) === undefined ? [] : [String(node['name'])];
  if (node.type !== 'MemberExpression' || node['computed'] === true) return [];
  const left = member(node['object']); const right = name(node['property']);
  return left.length === 0 || right === undefined ? [] : [...left, right];
}
