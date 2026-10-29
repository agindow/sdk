import { children, name, parse, syntax } from '../report/ast.js';
import type { Syntax } from '../report/ast.js';
import { record } from './input.js';
import { abstain } from './types.js';
import type { BindingResult, BindingSelection, SourceSpan } from './types.js';
export function span(source: string, node: Syntax): SourceSpan {
  const before = source.slice(0, node.start).split('\n');
  return { start: node.start, end: node.end, line: before.length, column: (before.at(-1)?.length ?? 0) + 1 };
}
function topFunctions(tree: Syntax, symbol: string): Syntax[] {
  const result: Syntax[] = [];
  for (const statement of children(tree)) {
    const node = statement.type === 'ExportNamedDeclaration' && syntax(statement['declaration']) ? statement['declaration'] : statement;
    if (node.type === 'FunctionDeclaration' && name(node['id']) === symbol) result.push(node);
    if (node.type === 'VariableDeclaration' && node['kind'] === 'const') for (const declaration of children(node)) {
      if (name(declaration['id']) === symbol && syntax(declaration['init'])) result.push(declaration['init']);
    }
  }
  return result;
}
function directAccess(value: unknown, parameter: string): value is Syntax {
  return syntax(value) && value.type === 'MemberExpression' && value['computed'] !== true && value['optional'] !== true
    && syntax(value['object']) && value['object'].type === 'Identifier' && name(value['object']) === parameter
    && syntax(value['property']) && value['property'].type === 'Identifier';
}
/** Deliberately small grammar: no alias/flow inference and no import resolution. */
export function resolveSpan(source: string, selection: BindingSelection): SourceSpan | BindingResult {
  try {
    const tree = parse(source);
    const functions = topFunctions(tree, selection.symbol);
    if (functions.length !== 1) return abstain(functions.length > 1 ? 'ambiguous' : 'unsupported_syntax', functions.length > 0
      ? functions.map(node => span(source, node)) : [span(source, tree)]);
    const fn = functions[0];
    if (fn === undefined) return abstain('unsupported_syntax');
    const locations = [span(source, fn)];
    if (!['FunctionDeclaration', 'ArrowFunctionExpression', 'FunctionExpression'].includes(fn.type) || fn['generator'] === true || fn['async'] === true) return abstain('unsupported_syntax', locations);
    const params = fn['params']; const body = fn['body'];
    if (!Array.isArray(params) || params.length !== 1 || !record(params[0]) || params[0]['type'] !== 'Identifier' || !syntax(body)) return abstain('unsupported_syntax', locations);
    // acorn-typescript omits end on typed arrow parameters; no span is taken from it.
    const parameter = params[0]['name'];
    if (typeof parameter !== 'string') return abstain('unsupported_syntax', locations);
    let expression: unknown = body;
    if (body.type === 'BlockStatement') {
      const statements = body['body'];
      if (!Array.isArray(statements) || statements.length !== 1 || !syntax(statements[0]) || statements[0].type !== 'ReturnStatement') return abstain('unsupported_syntax', locations);
      expression = statements[0]['argument'];
    }
    if (selection.side === 'response') {
      if (!directAccess(expression, parameter) || name(expression['property']) !== selection.field || !syntax(expression['property'])) return abstain('unsupported_syntax', locations);
      return span(source, expression['property']);
    }
    if (!syntax(expression) || expression.type !== 'ObjectExpression' || !Array.isArray(expression['properties'])) return abstain('unsupported_syntax', locations);
    const matches: Syntax[] = [];
    for (const prop of expression['properties']) {
      if (!syntax(prop) || prop.type !== 'Property' || prop['computed'] === true || prop['method'] === true || prop['shorthand'] === true || prop['kind'] !== 'init'
        || !syntax(prop['key']) || !directAccess(prop['value'], parameter)) return abstain('unsupported_syntax', locations);
      if (name(prop['key']) === selection.field) matches.push(prop['key']);
    }
    if (matches.length !== 1) return abstain(matches.length > 1 ? 'ambiguous' : 'unsupported_syntax', matches.length > 0 ? matches.map(node => span(source, node)) : locations);
    const match = matches[0];
    return match === undefined ? abstain('unsupported_syntax', locations) : span(source, match);
  } catch { return abstain('unsupported_syntax'); }
}
