/** Git's quoted-path grammar uses octal UTF-8 bytes, not JSON Unicode escapes. */
function quotedPath(prefix: string, file: string): string {
  const quoted = [...Buffer.from(prefix + file)].map(byte => byte === 34 ? '\\"' : byte === 92 ? '\\\\'
    : byte >= 32 && byte <= 126 ? String.fromCharCode(byte) : '\\' + byte.toString(8).padStart(3, '0')).join('');
  return `"${quoted}"`;
}
function lines(text: string): string[] { return text.match(/[^\n]*\n|[^\n]+$/g) ?? []; }
function patchLine(prefix: string, line: string): string {
  return prefix + line + (line.endsWith('\n') ? '' : '\n\\ No newline at end of file\n');
}
/** One changed region with three context lines; source line endings remain bytes. */
export function unifiedPatch(file: string, original: string, changed: string): string {
  const before = lines(original), after = lines(changed);
  let first = 0;
  while (first < before.length && before[first] === after[first]) first++;
  if (first === before.length && first === after.length) return '';
  let oldEnd = before.length, newEnd = after.length;
  while (oldEnd > first && newEnd > first && before[oldEnd - 1] === after[newEnd - 1]) { oldEnd--; newEnd--; }
  const start = Math.max(0, first - 3), context = Math.min(3, before.length - oldEnd);
  const oldCount = oldEnd + context - start, newCount = newEnd + context - start;
  let patch = `--- ${quotedPath('a/', file)}\n+++ ${quotedPath('b/', file)}\n@@ -${start + 1},${oldCount} +${start + 1},${newCount} @@\n`;
  for (const line of before.slice(start, first)) patch += patchLine(' ', line);
  for (const line of before.slice(first, oldEnd)) patch += patchLine('-', line);
  for (const line of after.slice(first, newEnd)) patch += patchLine('+', line);
  for (const line of before.slice(oldEnd, oldEnd + context)) patch += patchLine(' ', line);
  return patch;
}
