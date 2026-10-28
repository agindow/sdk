/** Build-time editorial records, never runtime guard policy. */
export interface DocTable {
  readonly vocabulary?: 'modes' | 'decisions';
  readonly columns: readonly string[];
  readonly rows: readonly (readonly string[])[];
}
export interface DocSection {
  readonly heading: string; readonly aliases?: readonly string[]; readonly text: string; readonly schema?: string;
  readonly example?: { readonly language: DocCodeLanguage; readonly code: string };
  readonly links?: readonly { readonly label: string; readonly href: string }[];
  readonly unavailable?: readonly string[];
  readonly table?: DocTable;
}
/** Language key for tab chrome and highlighter (Better Auth–style short labels). */
export type DocCodeLanguage = 'ts' | 'python' | 'bash' | 'json' | 'sh';
export interface DocCodeSample {
  readonly label: string;
  readonly language: DocCodeLanguage;
  readonly matrix: string;
  readonly code: string;
}
/** One rendered code block; SDK samples pair TypeScript + Python tabs. */
export interface DocSnippet {
  readonly samples: readonly DocCodeSample[];
}
export interface DocPage {
  readonly family: string; readonly version: string; readonly slug: string;
  readonly path?: string;
  /** Short task-oriented sidebar label; the full title remains searchable. */
  readonly navTitle?: string;
  readonly title: string; readonly published: boolean; readonly sections: readonly DocSection[];
  readonly loop?: boolean;
  readonly snippets?: readonly DocSnippet[];
}
export interface DocsTree { readonly format: 'agindow.docs/1'; readonly revision: string; readonly pages: readonly DocPage[] }
export const docsRevision = 'docs-v1-2026-10-28-public-developer-documents';
export const modes: DocTable = { vocabulary: 'modes', columns: ['Mode', 'Repair', 'Incomplete repair / ambiguity'], rows: [
  ['observe', 'Never mutates', 'Log and audit only; preserves upstream outcomes.'],
  ['repair', 'Selective, verified and confidence-gated', 'Reads fail open with degraded; semantic writes fail closed. Shape drift passes through; unknown writes default fail closed, with explicit failClosed opt-out; malformed whole bodies pass through without repair.'],
  ['strict', 'Selective, verified', 'Incomplete repair fails closed; confidence below 0.99 halts for review. Ignores failClosed opt-out. Infrastructure failures still bypass.'],
] };
export const decisions: DocTable = { vocabulary: 'decisions', columns: ['Decision', 'Meaning'], rows: [
  ['ok', 'No actionable drift reported; this alone does not prove protection was configured.'],
  ['repaired', 'A verified selective patch was applied; bytes may change and byte_identical is false.'],
  ['degraded', 'Policy permits the original read with a loud drift warning.'],
  ['bypassed', 'Inspection unavailable or skipped; original outcome preserved.'],
  ['blocked', 'Policy refusal; distinguish request refusal from a completed response halt.'],
] };
/** Public substitute for the private runtime-contract blob; anchors are ignored. */
export const contract = (_anchor: string) => ({ label: 'Modes', href: '/docs/modes' });
