import { parseCredentialEvent, notificationText } from 'contracts';
import type { CredentialEvent } from 'contracts';
export interface Options { readonly now: string; readonly staleMs: number; readonly connectionCap: number; readonly operationCap: number }
interface Fact {
  tenant: string; connection: string; observed_at: string; source: string;
  dual_key: 'yes' | 'no' | 'unknown'; mint: 'manual' | 'api' | 'none' | 'unknown'; replacement: boolean;
  issued_at?: string; scopes?: readonly string[];
}
export interface Row {
  tenant: string; connection: string; verdict: 'verifiable' | 'flag_day' | 'unsupported' | 'unknown'; reason: string;
  health: string; last_ok: string | null; observed_since: string | null; observed_operations: readonly string[];
  auth_breadth: 'unknown' | { used_operations: readonly string[]; granted_scopes: readonly string[] };
  mint: Fact['mint']; dual_key: Fact['dual_key']; sources: readonly string[];
  provenance: Readonly<Record<string, readonly string[]>>; warning?: string; issued_at?: string;
}
interface Recent { event: CredentialEvent; ts: string; sortKey: string; order: number }
interface Accumulator { tenant: string; connection: string; first: string | null; operations: Set<string>; recent: Recent[]; lastOk: string | null; lastOkVersion: string | null; activeVersion: string | null; rotationAt: string | null }
function invalid(): never { throw new Error('Invalid rotation report input'); }
function utc(value: unknown): string {
  if (typeof value !== 'string' || value.length > 30 || !value.endsWith('Z') || !Number.isFinite(Date.parse(value))) invalid();
  return new Date(value).toISOString();
}
function text(value: unknown): string { if (typeof value !== 'string') invalid(); return notificationText(value); }
function fact(value: unknown): Fact {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) invalid();
  const v = value as Record<string, unknown>;
  if (Object.keys(v).some(k => !['tenant','connection','observed_at','source','dual_key','mint','replacement','issued_at','scopes'].includes(k))) invalid();
  const dual = v['dual_key'], mint = v['mint'];
  if (dual !== 'yes' && dual !== 'no' && dual !== 'unknown') invalid();
  if (mint !== 'manual' && mint !== 'api' && mint !== 'none' && mint !== 'unknown') invalid();
  if (typeof v['replacement'] !== 'boolean') invalid();
  const scopes = v['scopes'];
  if (scopes !== undefined && (!Array.isArray(scopes) || scopes.length > 256)) invalid();
  return { tenant: text(v['tenant']), connection: text(v['connection']), source: text(v['source']), observed_at: utc(v['observed_at']), dual_key: dual, mint, replacement: v['replacement'],
    ...(v['issued_at'] === undefined ? {} : { issued_at: utc(v['issued_at']) }),
    ...(scopes === undefined ? {} : { scopes: (scopes as unknown[]).map(text).sort() }) };
}
const key = (tenant: string, connection: string): string => JSON.stringify([tenant, connection]);
function eventTime(value: string): string {
  const whole = value.slice(0, -1), dot = whole.indexOf('.');
  return dot < 0 ? `${whole}.000000000Z` : `${whole.slice(0, dot)}.${whole.slice(dot + 1).padEnd(9, '0')}Z`;
}
/** Offline streaming aggregation: O(connectionCap * operationCap), independent
 * of event count. Input is NDJSON; event bytes and fact count are separately capped. */
export async function rotationReport(events: AsyncIterable<unknown>, rawFacts: readonly unknown[], options: Options): Promise<{ schema: 'agindow.rotation-readiness/1'; as_of: string; rows: readonly Row[] }> {
  const now = utc(options.now);
  if (![options.connectionCap, options.operationCap, options.staleMs].every(n => Number.isSafeInteger(n) && n > 0) || options.connectionCap > 65536 || options.operationCap > 1024 || rawFacts.length > options.connectionCap * 16) invalid();
  const facts = new Map<string, Fact[]>();
  for (const raw of rawFacts) { const f = fact(raw), k = key(f.tenant, f.connection); const group = facts.get(k) ?? []; if (group.length >= 16) invalid(); group.push(f); facts.set(k, group); }
  const records = new Map<string, Accumulator>();
  let order = 0;
  for await (const raw of events) {
    if (!Number.isSafeInteger(++order)) invalid();
    const e = parseCredentialEvent(raw); const k = key(e.tenant, e.connection);
    let r = records.get(k);
    if (r === undefined) { if (records.size >= options.connectionCap) invalid(); r = { tenant: e.tenant, connection: e.connection, first: utc(e.ts), operations: new Set(), recent: [], lastOk: null, lastOkVersion: null, activeVersion: null, rotationAt: null }; records.set(k, r); }
    const ts = utc(e.ts); if (r.first === null || ts < r.first) r.first = ts;
    r.operations.add(e.operation); if (r.operations.size > options.operationCap) invalid();
    const sortKey = eventTime(e.ts);
    if (e.cause === 'rotated' && e.credential_version !== undefined && (r.rotationAt === null || sortKey >= r.rotationAt)) {
      r.activeVersion = e.credential_version;
      r.rotationAt = sortKey;
      r.recent = r.recent.filter(previous => previous.event.credential_version === r.activeVersion);
      if (r.lastOkVersion !== r.activeVersion) {
        r.lastOk = r.recent.reduce<string | null>((latest, previous) => previous.event.cause === 'recovered' && (latest === null || previous.ts > latest) ? previous.ts : latest, null);
        r.lastOkVersion = r.lastOk === null ? null : r.activeVersion;
      }
    }
    if (r.activeVersion !== null && e.credential_version !== r.activeVersion) continue;
    if (e.cause === 'recovered' && (r.lastOk === null || ts > r.lastOk)) {
      r.lastOk = ts; r.lastOkVersion = e.credential_version ?? null;
    }
    if (e.cause !== "quota" && !r.recent.some(previous => previous.event.id === e.id)) {
      r.recent.push({ event: e, ts, sortKey, order });
      r.recent.sort((a,b) => b.sortKey.localeCompare(a.sortKey) || b.order - a.order);
      r.recent.length = Math.min(r.recent.length, 3);
    }
  }
  for (const [k,group] of facts) {
    const f = group[0];
    if (f !== undefined && !records.has(k)) {
      if (records.size >= options.connectionCap) invalid();
      records.set(k,{tenant:f.tenant,connection:f.connection,first:null,operations:new Set(),recent:[],lastOk:null,lastOkVersion:null,activeVersion:null,rotationAt:null});
    }
  }
  const rows: Row[] = [];
  for (const [k, r] of [...records].sort(([a],[b]) => a.localeCompare(b))) {
    const e = r.recent[0]?.event;
    const group = [...(facts.get(k) ?? [])].sort((a,b) => a.source.localeCompare(b.source) || a.observed_at.localeCompare(b.observed_at));
    const sources = [...new Set(group.map(f => f.source))];
    const f = group[0], ops = [...r.operations].sort();
    const disputed = group.some(other => other.dual_key !== f?.dual_key || other.mint !== f.mint || other.replacement !== f.replacement || JSON.stringify(other.scopes) !== JSON.stringify(f.scopes) || other.issued_at !== f.issued_at);
    const stale = group.some(f => Date.parse(now) - Date.parse(f.observed_at) > options.staleMs || f.observed_at > now);
    let verdict: Row['verdict'] = 'unknown', reason = 'no_capability_facts';
    if (f !== undefined) {
      if (disputed) reason = 'disputed_facts'; else if (stale) reason = 'stale_facts';
      else if (!f.replacement) { verdict = 'unsupported'; reason = 'no_replacement'; }
      else if (f.dual_key === 'no') { verdict = 'flag_day'; reason = 'atomic_invalidation'; }
      else if (f.dual_key === 'yes') { verdict = 'verifiable'; reason = 'coexisting_credentials'; }
      else reason = 'unknown_dual_key';
    }
    const failure = (cause: string): boolean => ['rejected','expired','revoked','scope_reduced'].includes(cause);
    const terminal = e !== undefined && ['expired','revoked'].includes(e.cause) && ['provider_declared','host_declared'].includes(e.evidence);
    const ambiguous = r.activeVersion === null && new Set(r.recent.map(previous => previous.event.credential_version ?? null)).size > 1;
    const health = ambiguous ? 'unknown' : e?.cause === 'recovered' ? 'healthy' : terminal || r.recent.every(previous => failure(previous.event.cause)) && r.recent.length === 3 ? 'failing' : e !== undefined && (failure(e.cause) || e.cause === 'rotated') ? 'degrading' : 'unknown';
    const observed = ['credential_event export'];
    rows.push({ tenant: r.tenant, connection: r.connection, verdict, reason, health, last_ok: r.lastOk, observed_since: r.first, observed_operations: ops,
      auth_breadth: !disputed && !stale && f?.scopes !== undefined ? { used_operations: ops, granted_scopes: f.scopes } : 'unknown', mint: f?.mint ?? 'unknown', dual_key: f?.dual_key ?? 'unknown', sources,
      provenance: { verdict: sources, health: observed, last_ok: observed, observed_since: observed, observed_operations: observed, auth_breadth: f?.scopes === undefined ? [] : sources, mint: sources, dual_key: sources, issued_at: f?.issued_at === undefined ? [] : sources },
      ...(verdict === 'flag_day' ? { warning: 'Atomic invalidation: cutover cannot be verified without downtime.' } : {}),
      ...(f?.issued_at === undefined || disputed || stale ? {} : { issued_at: f.issued_at }) });
  }
  return { schema: 'agindow.rotation-readiness/1', as_of: now, rows };
}
export function table(rows: readonly Row[]): string {
  return ['tenant\tconnection\tverdict\treason\thealth\tlast_ok\tobserved_since (lower bound)\tobserved_operations\tauth_breadth\tmint\tdual_key\tprovenance',
    ...rows.map(r => [r.tenant,r.connection,r.verdict,r.reason,r.health,r.last_ok ?? 'unknown',r.observed_since ?? 'unknown',r.observed_operations.join(','),JSON.stringify(r.auth_breadth),r.mint,r.dual_key,JSON.stringify(r.provenance),r.warning ?? '',r.issued_at === undefined ? '' : `issued_at=${r.issued_at}`].join('\t'))].join('\n');
}
