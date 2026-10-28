/** Constant-time bearer comparison for the private operator transport. */
export function authorize(header: string | null, token: string): boolean {
  if (!header || !token || token.length < 32 || token.length > 256) return false;
  const prefix = 'Bearer ';
  if (!header.startsWith(prefix)) return false;
  const presented = header.slice(prefix.length);
  if (presented.length !== token.length) return false;
  let diff = 0;
  for (let i = 0; i < token.length; i += 1) diff |= presented.charCodeAt(i) ^ token.charCodeAt(i);
  return diff === 0;
}
