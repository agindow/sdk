import {type CustomerWorkloads} from '../../owner/admission/worker.js';
import { WorkloadAdmission } from '../../owner/admission/service.js';
export { CustomerWorkloads } from '../../owner/admission/worker.js';
import { resetNative } from './native-switch.js';
import { KeylessExecutionOwner } from '../../owner/owner.js';
declare const FIXTURE_CLOCK_START: number;
let time = FIXTURE_CLOCK_START;
Date.now = () => time * 1000;
import ingress from '../../owner/worker.js';
import job from '../../owner/control/job.js';
import { KeylessPolicyAuthority } from '../../owner/control/authority.js';
export { Renewal, Administration, MultiAdministration, MultiRenewal } from '../../owner/control/worker.js';
export default {
  async fetch(request:Request, env:KeylessOwnerBindings & KeylessControlBindings & KeylessRenewalBindings & {FIXTURE_ROLE?:string;CUSTOMER_WORKLOADS:Service<CustomerWorkloads>}):Promise<Response> {
    if(env.FIXTURE_ROLE==='customer'){
      const {identity,id,input}=await request.json() as {identity:string;id?:string;input?:string};
      try{return new Response(input!==undefined&&id!==undefined?await env.CUSTOMER_WORKLOADS.mutate(identity,id,input):await env.CUSTOMER_WORKLOADS.list(identity));}
      catch{return new Response(null,{status:403});}
    }
    if(env.FIXTURE_ROLE==='control') return new Response(null,{status:404});
    if(env.FIXTURE_ROLE==='job') {
      if(new URL(request.url).pathname==='/run') {
        await job.scheduled({scheduledTime:Date.now(),cron:'*/10 * * * *',noRetry(){}},env);
        return new Response(await env.RENEWAL.health());
      }
      if(new URL(request.url).pathname==='/write') {
        try {
          const write:unknown=Reflect.get(env.RENEWAL,'submit');
          if(typeof write!=='function') return new Response(null,{status:403});
          await Reflect.apply(write,env.RENEWAL,['{}']);
          return new Response(null,{status:500});
        } catch { return new Response(null,{status:403}); }
      }
      return job.fetch();
    }
    return ingress.fetch(request,env);
  },
};
export class FixturePolicy extends KeylessPolicyAuthority {
  #activity=0;
  #loseAck=false;
  #competing:string|undefined;
  #afterCommit:string|undefined;
  advanceAfterCommit(raw:string):void{this.#afterCommit=raw;}
  competeOnce(raw:string):void{this.#competing=raw;}
  loseNextAck():void {this.#loseAck=true;}
  activity():number {return this.#activity;}
  override inspect():Promise<string> {this.#activity++;return super.inspect();}
  override status():Promise<string> {this.#activity++;return super.status();}
  override async submit(raw:string):Promise<string> {this.#activity++;if(this.#competing){const prior=this.#competing;this.#competing=undefined;await super.submit(prior);}const result=await super.submit(raw);if(this.#afterCommit){const next=this.#afterCommit;this.#afterCommit=undefined;await super.submit(next);}if(this.#loseAck){this.#loseAck=false;throw Error('fixture lost ack');}return result;}
  override renewCurrent():Promise<string> {this.#activity++;return super.renewCurrent();}
  override deliver():Promise<void> {this.#activity++;return super.deliver();}
  setTime(value:number):void {time=value;}
  alarmAt():Promise<number|null> {return this.ctx.storage.getAlarm();}
  runAlarm():Promise<void> {return super.alarm();}
  configureKeys(raw:string):void {this.env.KEYLESS_REVIEW_KEYS=raw;}
  eraseControl():void { this.ctx.storage.sql.exec('DROP TABLE registration_control'); this.ctx.storage.sql.exec('DROP TABLE registration_commands'); }
  restoreControl(raw:string):void { this.ctx.storage.sql.exec('UPDATE registration_control SET body=? WHERE id=1',raw); }
  corruptRows():void {
    const row=this.ctx.storage.sql.exec<{body:string}>('SELECT body FROM registration_control WHERE id=1').one();
    const state=JSON.parse(row.body) as import('../../owner/control/state.js').ControlState;
    if(state.snapshot) state.snapshot.review.payload.rows=state.snapshot.review.payload.rows.map((r,i)=>i===0 ? {...r,tenant:'forged'} : r);
    this.restoreControl(JSON.stringify(state));
  }
  fillCommands(until:number):void {
    this.ctx.storage.transactionSync(()=>{ for(let i=0;i<1024;i++) this.ctx.storage.sql.exec('INSERT OR IGNORE INTO registration_commands VALUES(?,?,?,?)','fixture-'+i,'digest',until,'receipt'); });
  }
}
/** Fixture-only clock and fault ports; production exports no such methods. */
export class FixtureOwner extends KeylessExecutionOwner {
  #partitioned = false;
  #deliveryDown = false;
  deliveryDown(value:boolean):void { this.#deliveryDown=value; }
  override installPolicy(raw:string,cap:string,id:string):Promise<string> {
    if(this.#deliveryDown) return Promise.reject(Error('fixture delivery partition'));
    return super.installPolicy(raw,cap,id);
  }
  eraseGuardSql():void {
    this.eraseLedger(); this.ctx.storage.sql.exec('DROP TABLE owner_policy');
  }
  partition(value: boolean): void { this.#partitioned = value; }
  override async execute(request: Request, registration: string): Promise<Response> {
    if (this.#partitioned) throw Error('fixture authority partition');
    return super.execute(request, registration);
  }
  counters(): string { return JSON.stringify(this.admissionCounters()); }
  configureKeys(raw:string):void {if('KEYLESS_REVIEW_KEYS' in this.env) this.env.KEYLESS_REVIEW_KEYS=raw;}
  setMode(mode:string):void {this.env.KEYLESS_POLICY_MODE=mode;}
  setEnabled(enabled:string):void {this.env.KEYLESS_ENABLED=enabled;}
  configure(raw: string): void { if('KEYLESS_REGISTRATIONS' in this.env) this.env.KEYLESS_REGISTRATIONS = raw; }
  setTime(value: number): void {
    time = value;
  }
  resetVerifier(): void {
    resetNative();
  }
  eraseLedger(): void {
    this.ctx.storage.sql.exec('DROP TABLE assertion_fence');
    this.ctx.storage.sql.exec('DROP TABLE owner_clock');
  }
  fillLedger(until: number): void {
    this.ctx.storage.transactionSync(() => {
      for (let i = 0; i < 1024; i++)
        this.ctx.storage.sql.exec(
          'INSERT OR IGNORE INTO assertion_fence VALUES (?,?)',
          'fixture-' + i,
          until,
        );
    });
  }
  retained(): number {
    return this.ctx.storage.sql
      .exec<{ count: number }>('SELECT count(*) AS count FROM assertion_fence')
      .one().count;
  }
}

export class FixtureAdmission extends WorkloadAdmission {
 readonly #signing=this.env.ADMISSION_SIGNING_JWK;
 setFeatureEnabled(enabled:boolean):void{this.env.FEATURE_FLAGS=enabled?'workload-self-serve':'';}
 setSigningEnabled(enabled:boolean):void{this.env.ADMISSION_SIGNING_JWK=enabled?this.#signing:'{}';}
 configurePolicy(raw:string):void{this.env.ADMISSION_POLICY=raw;}
 configureCatalog(raw:string):void{if('KEYLESS_READ_CATALOG' in this.env)this.env.KEYLESS_READ_CATALOG=raw;}
 setTime(value:number):void{time=value;}
 eraseAdmission():void{for(const table of ['workload_intents','workloads','workload_clock'])this.ctx.storage.sql.exec('DELETE FROM '+table);}
 stored():string{return JSON.stringify(this.ctx.storage.sql.exec('SELECT body FROM workload_intents').toArray());}
 runAlarm():Promise<void>{return super.alarm();}
}

import {OrgProfileDirectory} from '../../owner/control/org-directory.js';
export class FixtureDirectory extends OrgProfileDirectory {
 enrolled():number{return this.ctx.storage.sql.exec<{n:number}>('SELECT count(*) AS n FROM org_profiles WHERE generation IS NOT NULL').one().n;}
 erase():void{this.ctx.storage.sql.exec('DELETE FROM org_profiles');}
 runAlarm():Promise<void>{return super.alarm();}
}
