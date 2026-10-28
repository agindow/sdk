export interface DeliveryRetry { id:string; at:number }
/** Failed owners wait 60s; healthy backlog must drain before the next renewal resets its lease. */
export function deliveryPlan(pending:readonly string[], retry:readonly DeliveryRetry[], now:number):{batch:string[];delay:number|null} {
  const ready=pending.filter(id=>(retry.find(r=>r.id===id)?.at ?? 0)<=now);
  const next=pending.map(id=>retry.find(r=>r.id===id)?.at ?? now);
  return {batch:ready.slice(0,16),delay:pending.length===0 ? null : ready.length ? 1 : Math.max(1,Math.min(60,Math.min(...next)-now))};
}
/** Previously undelivered owners keep priority when a new lease is issued. */
export function renewalTargets(pending:readonly string[], ids:readonly string[]):string[] {
  return [...new Set([...pending.filter(id=>ids.includes(id)),...ids])];
}
