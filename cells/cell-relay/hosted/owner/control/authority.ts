import { hostPolicy, isHostPolicy, orgProfile, scopedHostPolicy } from './host-policy.js';
import { admissionConstraint, admissionPolicy } from './admission-policy.js';
import { record } from '../policy.js';
import { DurableObject } from 'cloudflare:workers';
import { digest } from '../policy.js';
import { deliveryPlan, renewalTargets } from './delivery-plan.js';
import { integer, lease, verified } from './protocol.js';
import { health, reason, transition } from './state.js';
import { ControlStore } from './store.js';
export class KeylessPolicyAuthority extends DurableObject<KeylessControlBindings|KeylessMultiControlBindings> {
  #operations: Promise<unknown> = Promise.resolve();
  #delivery: Promise<unknown> = Promise.resolve();
  readonly #store: ControlStore;
  constructor(ctx: DurableObjectState, env: KeylessControlBindings|KeylessMultiControlBindings) {
    super(ctx,env);
    this.#store = new ControlStore(ctx.storage,this.now());
  }
  protected now(): number { return Math.floor(Date.now()/1000); }
  #serial<T>(work:()=>Promise<T>): Promise<T> {
    const pending = this.#operations.then(work);
    this.#operations = pending.catch(()=>undefined);
    return pending;
  }
  #profile():string {
    if('KEYLESS_HOST_POLICY' in this.env){
      const binding=this.#store.orgBinding();
      if(!binding||binding.generation!==this.#store.read().generation)throw Error('org_binding');return binding.profile;
    }
    return this.env.KEYLESS_PROFILE;
  }
  #enabled():void {if(this.env.KEYLESS_CONTROL_ENABLED!=='true'||!this.ctx.id.equals(this.env.POLICIES.idFromName(this.#profile())))throw Error('authority_disabled');}
  async openOrg(tenant:string,nonce:string):Promise<string>{
    if(!('KEYLESS_HOST_POLICY' in this.env)||this.env.KEYLESS_CONTROL_ENABLED!=='true')throw Error('authority_disabled');
    const p=hostPolicy(JSON.parse(this.env.KEYLESS_HOST_POLICY)),profile=await orgProfile(p,tenant);
    if(p.origin!==this.env.EXECUTION_ORIGIN||p.until<=this.now()||!this.ctx.id.equals(this.env.POLICIES.idFromName(profile)))throw Error('org_binding');
    return this.#serial(async()=>{const fresh=this.#store.bindOrg(tenant,profile,nonce);return JSON.stringify({fresh,state:this.#store.read()});});
  }
  async status(): Promise<string> { this.#enabled(); return health(this.#store.read()); }
  async inspect(): Promise<string> { this.#enabled(); return JSON.stringify(this.#store.read()); }
  async submit(raw: string): Promise<string> {
    this.#enabled();
    const result = await this.#serial(async()=>{
      const now = this.now(), c = await verified(raw,this.env.KEYLESS_REVIEW_KEYS,now,true), state = this.#store.read();
      if (c.payload.origin!==this.env.EXECUTION_ORIGIN || c.payload.profile!==this.#profile()) throw Error('review_scope');
      if(now<state.floor) throw Error('clock');
      const commandDigest = await digest(JSON.stringify(c));
      const prior = this.#store.receipt(c.payload.id,commandDigest,now);
      if (prior !== undefined) return prior;
      const keys:unknown=JSON.parse(this.env.KEYLESS_REVIEW_KEYS);
      if(!Array.isArray(keys))throw Error('review_keys');
      const key=keys.map(record).find(k=>k.kid===c.payload.kid);
      if('KEYLESS_HOST_POLICY' in this.env){
        const binding=this.#store.orgBinding();if(!binding)throw Error('org_binding');
        const scope=await scopedHostPolicy(hostPolicy(JSON.parse(this.env.KEYLESS_HOST_POLICY)),this.#profile(),c.payload.rows,state.generation);
        if(scope.tenant!==binding.tenant)throw Error('admission_scope');
      }
      if(key && 'policy' in key){const p=isHostPolicy(key.policy)?await scopedHostPolicy(hostPolicy(key.policy),c.payload.profile,c.payload.rows,state.generation):admissionPolicy(key.policy);admissionConstraint(p,c.payload,state,now);}
      const next = await transition(state,c,now), receipt = health(next);
      this.#store.commit(next,c.payload.id,commandDigest,c.payload.exp+30,receipt,now);
      return receipt;
    });
    await this.#arm(1);
    return result;
  }
  /** Read-only proof of a past commit; never authorizes another lease or command. */
  async recover(raw:string):Promise<string> {
    this.#enabled();
    return this.#serial(async()=>{
      const now=this.now(),c=await verified(raw,this.env.KEYLESS_REVIEW_KEYS,now,false),state=this.#store.read();
      if(c.payload.origin!==this.env.EXECUTION_ORIGIN||c.payload.profile!==this.#profile()||c.payload.generation!==state.generation||now<state.floor)throw Error('review_scope');
      // Absence proves non-commit only while the durable command fence is retained.
      if(now>=c.payload.exp+30)throw Error('receipt_expired');
      return this.#store.receipt(c.payload.id,await digest(JSON.stringify(c)),now)??'null';
    });
  }
  async renewCurrent(): Promise<string> {
    this.#enabled();
    let failedRevision:number|undefined;
    try {
      await this.#serial(async()=>{
        const state = this.#store.read(), now = integer(this.now());
        failedRevision=state.revision;
        if (now<state.floor) throw Error('clock');
        const snapshot = state.snapshot;
        if (!snapshot) throw Error('review_required');
        if (snapshot.status!=='active') throw Error('review_inactive');
        if (now>=snapshot.notAfter || now>=snapshot.review.payload.reviewUntil) {
          this.#store.write({...state,floor:now,snapshot:{...snapshot,status:'expired'}});
          throw Error('review_expired');
        }
        await verified(JSON.stringify(snapshot.review),this.env.KEYLESS_REVIEW_KEYS,now,false);
        const renewal = await lease({status:snapshot.status,until:snapshot.review.payload.reviewUntil,
          issuedAt:snapshot.issuedAt,notAfter:snapshot.notAfter,sequence:snapshot.sequence,
          digest:snapshot.review.payload.digest,rows:snapshot.review.payload.rows},now,this.env.EXECUTION_ORIGIN,this.#profile(),this.#interval());
        this.#store.write({...state,floor:now,snapshot:{...snapshot,...renewal},
          pending:renewal.sequence===snapshot.sequence ? state.pending : renewalTargets(state.pending,snapshot.review.payload.rows.map(r=>r.id))});
      });
      await this.deliver();
      return health(this.#store.read());
    } catch (error) {
      const failure = reason(error);
      try { await this.#serial(async()=>{ const state=this.#store.read(); if(state.revision!==failedRevision) return; this.#store.write({...state,lastFailure:failure,
        snapshot:failure==='review_drift' && state.snapshot ? {...state.snapshot,status:'stopped'} : state.snapshot}); }); } catch { /* Preserve unavailable on storage fault. */ }
      console.info(JSON.stringify({event:'policy_renewal',outcome:'unavailable',reason:failure}));
      throw Error('renewal_unavailable');
    } finally { await this.#scheduleDelivery(); }
  }
  #interval():number { const seconds=Number(this.env.KEYLESS_RENEWAL_SECONDS); if(!Number.isSafeInteger(seconds)||seconds<60||seconds>600) throw Error('interval'); return seconds; }
  async #arm(seconds: number): Promise<void> {
    await this.ctx.storage.setAlarm((this.now()+seconds)*1000);
  }
  async deliver(): Promise<void> {
    const pending = this.#delivery.then(()=>this.#deliver());
    this.#delivery = pending.catch(()=>undefined);
    return pending;
  }
  async #deliver(): Promise<void> {
    this.#enabled();
    const start = this.#store.read();
    for (const id of deliveryPlan(start.pending,start.retry,this.now()).batch) {
      const current = this.#store.read(), snapshot = current.snapshot;
      if (!snapshot || current.revision!==start.revision || snapshot.sequence!==start.snapshot?.sequence) break;
      try {
        const name = await digest('agindow-owner-v1:'+id);
        const check=this.#store.read();
        if(check.revision!==current.revision || check.snapshot?.sequence!==snapshot.sequence) break;
        const ack = await this.env.EXECUTORS.getByName(name).installPolicy(JSON.stringify(snapshot),this.env.KEYLESS_DELIVERY_TOKEN,id);
        if (ack !== `${snapshot.review.payload.generation}:${snapshot.revision}:${snapshot.sequence}`) throw Error('delivery_ack');
        await this.#serial(async()=>{
          const latest = this.#store.read();
          if (latest.revision===current.revision && latest.snapshot?.sequence===snapshot.sequence)
            this.#store.write({...latest,pending:latest.pending.filter(target=>target!==id),retry:latest.retry.filter(r=>r.id!==id)});
        });
      } catch {
        await this.#serial(async()=>{
          const latest=this.#store.read();
          if(latest.revision===current.revision && latest.snapshot?.sequence===snapshot.sequence)
            this.#store.write({...latest,pending:[...latest.pending.filter(target=>target!==id),id],
              retry:[...latest.retry.filter(r=>r.id!==id),{id,at:this.now()+60}]});
        });
        console.info(JSON.stringify({event:'policy_delivery',outcome:'unavailable'}));
      }
    }
  }
  async #scheduleDelivery():Promise<void> {
    await this.#serial(async()=>{
      const state=this.#store.read(), plan=deliveryPlan(state.pending,state.retry,this.now());
      if(plan.delay===null) await this.ctx.storage.deleteAlarm();
      else await this.#arm(plan.delay);
    });
  }
  override async alarm(): Promise<void> {
    try { await this.deliver(); }
    catch { /* Delivery retries do not authorize a new lease. */ }
    finally {
      await this.#scheduleDelivery();
    }
  }
}
