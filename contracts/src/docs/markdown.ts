import type { DocPage, DocSnippet } from './model.js';
export const loopSteps = ['Wrap', 'Inspect', 'Frozen-mode decision: ok / repaired / degraded / bypassed / blocked', 'Audit artifact', 'Optional feedback'] as const;
function fenceLanguage(language: string): string {
  if (language === 'ts') return 'ts';
  if (language === 'python') return 'python';
  if (language === 'json') return 'json';
  if (language === 'text') return 'text';
  return 'sh';
}
const tableCell = (value: string): string => value.replaceAll('\\', '\\\\').replaceAll('|', '\\|').replaceAll('\n', '<br>');
function snippetMarkdown(snippet: DocSnippet): string {
  return snippet.samples.map(sample => `\n\`\`\`${fenceLanguage(sample.language)}\n${sample.code}\n\`\`\`\n`).join('');
}
function sectionSnippet(page: DocPage, index: number | undefined): string {
  const snippet = index === undefined ? undefined : page.snippets?.[index];
  return snippet ? snippetMarkdown(snippet) : '';
}
export function markdownPage(page: DocPage): string {
  return `# ${page.title}\n\n${page.path ?? `/docs/${page.family}/${page.version}/${page.slug}`}\n\n`
    + (page.loop ? `${loopSteps.join(' → ')}\n\n` : '')
    + page.sections.map(section => (section.aliases ?? []).map(alias => `<a id="${alias}"></a>\n`).join('') + `## ${section.heading}\n\n${section.text}\n`
      + (section.diagram ? `\n${section.diagram.steps.join(' → ')}\n` : '')
      + sectionSnippet(page, section.snippet)
      + (section.example ? `\n\`\`\`${fenceLanguage(section.example.language)}\n${section.example.code}\n\`\`\`\n` : '')
      + (section.schema ? `\n\`\`\`json\n${section.schema}\n\`\`\`\n` : '')
      + (section.table ? `\n| ${section.table.columns.map(tableCell).join(' | ')} |\n| ${section.table.columns.map(() => '---').join(' | ')} |\n`
        + section.table.rows.map(row => `| ${row.map(tableCell).join(' | ')} |\n`).join('') : '')
      + (section.links ?? []).map(link => `\n[${link.label}](${link.href})\n`).join('')
      + (section.unavailable ?? []).map(label => `\n${label} — unavailable\n`).join('')).join('\n')
    + (page.snippets ?? []).filter((_, index) => !page.sections.some(section => section.snippet === index)).map(snippetMarkdown).join('');
}
