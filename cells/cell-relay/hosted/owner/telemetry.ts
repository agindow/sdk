/** Closed fields only: never attach request URLs, headers, identifiers or error text. */
export type Admission = 'accepted' | 'rejected' | 'replay' | 'capacity' | 'unavailable';
export type AdmissionCounters = Record<Admission, number>;
export function emptyCounters(): AdmissionCounters {
  return { accepted: 0, rejected: 0, replay: 0, capacity: 0, unavailable: 0 };
}
export async function admission(response: Response): Promise<Admission> {
  // Native completion markers distinguish upstream errors from admission failures. Never inspect their bodies.
  if (['ok', 'degraded', 'repaired', 'bypassed'].includes(response.headers.get('x-agindow-status') ?? '')) return 'accepted';
  if (response.status >= 500) return 'unavailable';
  if (response.status === 401 || response.status === 429) {
    const value: unknown = await response.clone().json().catch(() => null);
    if (typeof value === 'object' && value !== null && 'error' in value &&
      ['identity_replay', 'identity_Replay', 'auth_Replay'].includes(String(value.error))) return 'replay';
    if (typeof value === 'object' && value !== null && 'error' in value &&
      ['identity_capacity', 'identity_Capacity', 'auth_Capacity'].includes(String(value.error))) return 'capacity';
  }
  return response.status >= 400 ? 'rejected' : 'accepted';
}
export function report(counters: AdmissionCounters, outcome: Admission, elapsedMs: number): void {
  // Successful traffic increments memory counters; failures expose the aggregate without per-call success logs.
  if (outcome === 'accepted') return;
  console.info(JSON.stringify({ event: 'owner_admission', outcome, ...counters, elapsed_ms: elapsedMs }));
}
export function ingressUnavailable(): void {
  console.info(JSON.stringify({ event: 'owner_ingress', unavailable: 1 }));
}
