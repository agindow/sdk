/** Backtick spans in docs prose → plain text or structured segments for HTML. */

export type DocsInlineSegment =
  | { readonly kind: 'text'; readonly value: string }
  | { readonly kind: 'code'; readonly value: string };

/** Strip editorial backticks for human textContent comparisons. */
export function plainDocsProse(text: string): string {
  return text.replace(/`([^`]+)`/gu, '$1');
}

/** Split `code` spans; unmatched text stays kind text. Empty input → one empty text segment. */
export function docsInlineSegments(text: string): readonly DocsInlineSegment[] {
  const segments: DocsInlineSegment[] = [];
  const pattern = /`([^`]+)`/gu;
  let cursor = 0;
  let match: RegExpExecArray | null = pattern.exec(text);
  while (match !== null) {
    if (match.index > cursor) {
      segments.push({ kind: 'text', value: text.slice(cursor, match.index) });
    }
    segments.push({ kind: 'code', value: match[1] ?? '' });
    cursor = match.index + match[0].length;
    match = pattern.exec(text);
  }
  if (cursor < text.length || segments.length === 0) {
    segments.push({ kind: 'text', value: text.slice(cursor) });
  }
  return segments;
}
