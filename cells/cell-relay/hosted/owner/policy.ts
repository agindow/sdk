export interface Registration {
  readonly id: string;
  readonly kid: string;
  readonly x: string;
  readonly audience: string;
  readonly tenant: string;
  readonly environment: 'test' | 'live';
  readonly connection: string;
  readonly tools: readonly string[];
  readonly owner: string;
}
export interface Policy {
  readonly issuer: string;
  readonly notAfter: number;
  readonly registrations: readonly Registration[];
}
export function record(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw Error('invalid');
  return value as Record<string, unknown>;
}
function fields(value: Record<string, unknown>, names: readonly string[]): void {
  if (Object.keys(value).sort().join(',') !== [...names].sort().join(',')) throw Error('invalid');
}
function literal(value: unknown): string {
  if (typeof value !== 'string' || !value || value.length > 256 || /[\s\u007f/*]/u.test(value))
    throw Error('invalid');
  return value;
}
function address(value: unknown): string {
  if (typeof value !== 'string' || !value || value.length > 2048 || /[\s\u007f*]/u.test(value))
    throw Error('invalid');
  return value;
}
export async function digest(value: string): Promise<string> {
  const bytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value));
  return [...new Uint8Array(bytes)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}
export function jwtPart(token: string, index: number): Record<string, unknown> {
  if (token.length > 16384) throw Error('invalid');
  const parts = token.split('.');
  if (parts.length !== 3) throw Error('invalid');
  const part = parts[index];
  if (!part || !/^[A-Za-z0-9_-]+$/u.test(part)) throw Error('invalid');
  return record(JSON.parse(atob(part.replace(/-/gu, '+').replace(/_/gu, '/'))));
}
export async function policy(raw: string, now: number): Promise<Policy> {
  if (new TextEncoder().encode(raw).byteLength > 262144) throw Error('invalid');
  const value = record(JSON.parse(raw));
  fields(value, ['issuer', 'not_after', 'registrations']);
  const issuer = address(value.issuer),
    url = new URL(issuer);
  if (url.protocol !== 'https:' || url.origin !== issuer) throw Error('invalid');
  const notAfter = value.not_after;
  if (
    typeof notAfter !== 'number' ||
    !Number.isSafeInteger(notAfter) ||
    notAfter <= now ||
    notAfter > now + 3600
  )
    throw Error('invalid');
  if (
    !Array.isArray(value.registrations) ||
    !value.registrations.length ||
    value.registrations.length > 256
  )
    throw Error('invalid');
  const ids = new Set<string>();
  const registrations: Registration[] = [];
  for (const input of value.registrations) {
    const row = record(input);
    fields(row, ['id', 'kid', 'x', 'audience', 'tenant', 'environment', 'connection', 'tools']);
    const id = literal(row.id),
      kid = literal(row.kid),
      x = literal(row.x),
      audience = address(row.audience),
      tenant = literal(row.tenant),
      connection = literal(row.connection),
      environment = row.environment;
    if (
      ids.has(id) ||
      !/^[-_A-Za-z0-9]{43}$/u.test(x) ||
      (environment !== 'test' && environment !== 'live') ||
      !Array.isArray(row.tools) ||
      !row.tools.length ||
      row.tools.length > 64
    )
      throw Error('invalid');
    const tools = row.tools.map(literal);
    if (new Set(tools).size !== tools.length) throw Error('invalid');
    ids.add(id);
    registrations.push({
      id,
      kid,
      x,
      audience,
      tenant,
      environment,
      connection,
      tools,
      owner: await digest('agindow-owner-v1:' + id),
    });
  }
  return { issuer, notAfter, registrations };
}
export async function readBody(request: Request, max: number): Promise<string> {
  const reader = request.body?.getReader();
  if (!reader) throw Error('invalid');
  let size = 0;
  const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: false });
  let text = '';
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      size += next.value.byteLength;
      if (size > max) {
        await reader.cancel();
        throw Error('too_large');
      }
      text += decoder.decode(next.value, { stream: true });
    }
    return text + decoder.decode();
  } finally {
    reader.releaseLock();
  }
}
export function json(status: number, error: string): Response {
  return Response.json(
    { error },
    {
      status,
      headers: {
        'cache-control': 'no-store',
        'content-security-policy': "default-src 'none'",
        'x-content-type-options': 'nosniff',
        'referrer-policy': 'no-referrer',
      },
    },
  );
}
export function tokenOwner(token: string): string | undefined {
  const kid = jwtPart(token, 0).kid;
  return typeof kid === 'string'
    ? /^owner\.([a-f0-9]{64})\.[A-Za-z0-9_-]{43}$/u.exec(kid)?.[1]
    : undefined;
}
