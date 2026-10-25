import { parse, visit, syntax, name, literal, property, member } from './ast.js';
import type { Syntax } from './ast.js';

export interface DetectedTool {
  readonly name: string;
  readonly file: string;
  readonly wrapped: boolean;
  readonly vendors: readonly string[];
}

export interface ScanOptions { readonly maxFiles?: number; readonly maxBytes?: number }

export interface SourceFile {
  readonly name: string;
  readonly text: string;
}

const supportedVendors = new Map([
  ['stripe', 'stripe'],
  ['@shopify/shopify-api', 'shopify'],
  ['jsforce', 'salesforce'],
  ['@salesforce/core', 'salesforce'],
  ['@slack/web-api', 'slack'],
  ['@octokit/rest', 'github'],
]);

const SOURCE_NAME = /\.[cm]?[jt]sx?$/u;

function imports(tree: Syntax): {
  vendors: Map<string, string>;
  agindow: Set<string>;
  adapters: Set<string>;
  factories: Set<string>;
  nango: Set<string>;
} {
  const vendors = new Map<string, string>();
  const agindow = new Set<string>();
  const adapters = new Set<string>();
  const factories = new Set<string>();
  const nango = new Set<string>();
  visit(tree, node => {
    if (node.type !== 'ImportDeclaration' || !syntax(node['source'])) return;
    const source: unknown = node['source']['value'];
    if (typeof source !== 'string') return;
    const specifiers = node['specifiers'];
    if (!Array.isArray(specifiers)) return;
    for (const specifier of specifiers) {
      if (!syntax(specifier)) continue;
      const local = name(specifier['local']);
      if (local === undefined) continue;
      if (source === 'agindow') {
        const imported = name(specifier['imported']);
        if (imported === 'agindow' || specifier.type === 'ImportNamespaceSpecifier') agindow.add(local);
        if (imported !== undefined && ['wrap', 'guard', 'toolAdapter', 'clientAdapter'].includes(imported)) {
          adapters.add(local);
        }
      } else if (source === 'nango' && ['createAction', 'createSync'].includes(name(specifier['imported']) ?? '')) {
        nango.add(local);
      } else if (source === '@langchain/core/tools' && name(specifier['imported']) === 'tool') {
        factories.add(local);
      } else if (supportedVendors.has(source)) {
        vendors.set(local, supportedVendors.get(source) ?? source);
      }
    }
  });
  return { vendors, agindow, adapters, factories, nango };
}

/** Static tool discovery. Never executes source. Throws on unsupported dynamic shapes. */
export function analyze(source: string, file: string): DetectedTool[] {
  const tree = parse(source);
  const bindings = imports(tree);
  const wrappedNames = new Set<string>();
  const wrappedNodes = new Set<Syntax>();
  const vendorVariables = new Map(bindings.vendors);
  visit(tree, node => {
    if (node.type === 'VariableDeclarator' && syntax(node['init']) && node['init'].type === 'NewExpression') {
      const constructor = name(node['init']['callee']);
      const id = name(node['id']);
      if (constructor !== undefined && id !== undefined && bindings.vendors.has(constructor)) {
        vendorVariables.set(id, bindings.vendors.get(constructor) ?? constructor);
      }
    }
    if (node.type !== 'CallExpression') return;
    const path = member(node['callee']);
    const direct = name(node['callee']);
    const wrapper = (direct !== undefined && bindings.adapters.has(direct))
      || (path.length === 2 && bindings.agindow.has(path[0] ?? '')
        && ['wrap', 'guard', 'toolAdapter', 'clientAdapter'].includes(path[1] ?? ''));
    if (!wrapper || !Array.isArray(node['arguments'])) return;
    const target: unknown = node['arguments'][0];
    const id = name(target);
    if (id !== undefined) wrappedNames.add(id);
    if (syntax(target)) wrappedNodes.add(target);
  });
  // Follow only static direct aliases/collection references, bounded by the
  // source size. Function bodies and arbitrary expressions are never evaluated.
  const variables = new Map<string, Syntax>();
  visit(tree, node => {
    if (node.type === 'VariableDeclarator' && syntax(node['init'])) {
      const id = name(node['id']);
      if (id !== undefined) variables.set(id, node['init']);
    }
  });
  for (let round = 0; round < 64; round++) {
    const before = wrappedNames.size;
    for (const id of [...wrappedNames]) {
      const initializer = variables.get(id);
      if (initializer === undefined) continue;
      const references = initializer.type === 'ArrayExpression' && Array.isArray(initializer['elements'])
        ? initializer['elements']
        : initializer.type === 'ObjectExpression' && Array.isArray(initializer['properties'])
          ? initializer['properties'].map(value => syntax(value) ? value['value'] : undefined)
          : [initializer];
      for (const reference of references) {
        if (syntax(reference) && reference.type === 'Identifier') wrappedNames.add(name(reference) ?? '');
      }
    }
    if (wrappedNames.size === before) break;
  }
  const result: DetectedTool[] = [];
  visit(tree, (node, ancestors) => {
    let execute: Syntax | undefined;
    let toolName: string | undefined;
    const enclosing = ancestors.at(-1);
    if (node.type === 'ObjectExpression') {
      execute = property(node, 'execute') ?? property(node, 'func');
      if (execute === undefined || !['ArrowFunctionExpression', 'FunctionExpression', 'Identifier'].includes(execute.type)) {
        return;
      }
      const declared = property(node, 'name') ?? property(node, 'id');
      if (declared !== undefined && literal(declared) === undefined) throw Error('dynamic tool identity is unsupported');
      toolName = literal(declared) ?? (enclosing?.type === 'Property' ? name(enclosing['key']) : undefined);
    } else if (node.type === 'Property' && node['computed'] !== true && syntax(node['value'])
      && ['ArrowFunctionExpression', 'FunctionExpression'].includes(node['value'].type)) {
      const container = ancestors.at(-2);
      const isTools = (enclosing !== undefined && wrappedNodes.has(enclosing)) || (container?.type === 'VariableDeclarator'
        && (wrappedNames.has(name(container['id']) ?? '') || /(?:^tools$|Tools$)/.test(name(container['id']) ?? '')));
      if (!isTools) return;
      execute = node['value'];
      toolName = name(node['key']);
    } else if (node.type === 'CallExpression' && bindings.nango.has(name(node['callee']) ?? '') && Array.isArray(node['arguments'])) {
      const definition: unknown = node['arguments'][0];
      if (!syntax(definition) || definition.type !== 'ObjectExpression') throw Error('unsupported Nango definition');
      execute = property(definition, 'exec');
      if (execute === undefined || !['ArrowFunctionExpression', 'FunctionExpression'].includes(execute.type)) {
        throw Error('unsupported Nango executor');
      }
      toolName = `nango/${file.replace(/\\/g, '/').replace(/\.[cm]?[jt]sx?$/, '')}`;
    } else if (node.type === 'CallExpression' && bindings.factories.has(name(node['callee']) ?? '') && Array.isArray(node['arguments'])) {
      const callback: unknown = node['arguments'][0];
      const options: unknown = node['arguments'][1];
      if (!syntax(callback) || !syntax(options)) throw Error('unsupported tool factory');
      execute = callback;
      toolName = literal(property(options, 'name'));
      if (toolName === undefined) throw Error('dynamic factory identity');
    } else return;
    if (execute === undefined || toolName === undefined) return;
    const vendors = new Set<string>();
    // This is the template's integration label, never inferred remote-call proof.
    if (toolName.startsWith('nango/')) {
      const integration = /(?:^|\/)integrations\/([^/]+)\/(?:actions|syncs)\//.exec(file.replace(/\\/g, '/'))?.[1];
      if (integration !== undefined) vendors.add(integration);
    }
    visit(execute, candidate => {
      if (candidate.type !== 'CallExpression') return;
      const path = member(candidate['callee']);
      const root = path[0];
      if (root !== undefined && vendorVariables.has(root)) vendors.add(vendorVariables.get(root) ?? root);
      if (root === 'fetch' && Array.isArray(candidate['arguments'])) {
        const url = literal(candidate['arguments'][0]);
        if (url !== undefined) {
          try {
            const host = new URL(url).hostname;
            for (const vendor of ['stripe', 'shopify', 'salesforce', 'slack', 'github']) {
              if (host === `${vendor}.com` || host.endsWith(`.${vendor}.com`)
                || (vendor === 'shopify' && host.endsWith('.myshopify.com'))) {
                vendors.add(vendor);
              }
            }
          } catch { /* dynamic or relative target is unknown */ }
        }
      }
    });
    const wrapped = [...ancestors, node].some(ancestor => wrappedNodes.has(ancestor)
      || (ancestor.type === 'VariableDeclarator' && wrappedNames.has(name(ancestor['id']) ?? '')));
    result.push({ name: toolName, file, wrapped, vendors: [...vendors].sort() });
  });
  return result;
}

/**
 * In-memory static inventory (browser-safe). Skips non-JS/TS names, oversize
 * files, and parse failures. Never executes source or touches the filesystem.
 */
export function scanSources(
  files: readonly SourceFile[],
  options: ScanOptions = {},
): { tools: DetectedTool[]; skipped: number } {
  const tools: DetectedTool[] = [];
  let skipped = 0;
  let counted = 0;
  const maxFiles = options.maxFiles ?? 10_000;
  const maxBytes = options.maxBytes ?? 1024 * 1024;
  for (const file of files) {
    const base = file.name.replace(/^.*[/\\]/u, '');
    if (!SOURCE_NAME.test(base)) {
      skipped++;
      continue;
    }
    const bytes = new TextEncoder().encode(file.text).byteLength;
    if (++counted > maxFiles || bytes > maxBytes) {
      skipped++;
      continue;
    }
    try {
      tools.push(...analyze(file.text, file.name.replace(/\\/g, '/')));
    } catch {
      skipped++;
    }
  }
  return { tools, skipped };
}
