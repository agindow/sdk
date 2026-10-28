import { registrationProfile } from './control/host-policy.js';
import { connectionsFor } from './control/read-catalog.js';
import { DurableObject } from 'cloudflare:workers';
import type { OwnerBindings } from './bindings.js';
import { accept, capability, loadProjection, type Projection } from './control/delivery.js';
import NativeHost from '@agindow/native-host';
import { admission, emptyCounters, report, type AdmissionCounters } from './telemetry.js';
import {
  digest,
  json,
  jwtPart,
  policy,
  readBody,
  record,
  type Policy,
  type Registration,
} from './policy.js';
export class KeylessExecutionOwner extends DurableObject<OwnerBindings> {
  readonly #counters = emptyCounters();
  readonly #activation = crypto.randomUUID();
  #projection: Projection | null = null;
  #updates: Promise<unknown> = Promise.resolve();
  #floor = 0;
  #readyAt = Number.POSITIVE_INFINITY;
  #exchanges: Promise<unknown> = Promise.resolve();
  constructor(ctx: DurableObjectState, env: OwnerBindings) {
    super(ctx, env);
    ctx.blockConcurrencyWhile(async () => {
      const sql = ctx.storage.sql,
        now = this.now();
      sql.exec(
        'CREATE TABLE IF NOT EXISTS owner_clock (id INTEGER PRIMARY KEY CHECK(id=1), birth INTEGER NOT NULL, floor INTEGER NOT NULL)',
      );
      sql.exec(
        'CREATE TABLE IF NOT EXISTS assertion_fence (id TEXT PRIMARY KEY, expires INTEGER NOT NULL)',
      );
      sql.exec('INSERT OR IGNORE INTO owner_clock VALUES (1, ?, ?)', now, now);
      const state = sql
        .exec<{ birth: number; floor: number }>('SELECT birth,floor FROM owner_clock WHERE id=1')
        .one();
      this.#projection = loadProjection(ctx.storage);
      this.#floor = Math.max(state.floor,this.#projection?.issuedAt ?? 0);
      this.#readyAt = state.birth + 360;
    });
  }
  /** Private background delivery only. The scheduler has neither this binding nor capability. */
  async installPolicy(raw: string, authorization: string, registration: string): Promise<string> {
    const operation = this.#updates.then(async()=>{
      if (this.env.KEYLESS_POLICY_MODE!=='auto' || raw.length>300000) throw Error('policy_mode');
      if(!('KEYLESS_DELIVERY_TOKEN' in this.env) || !('KEYLESS_REVIEW_KEYS' in this.env)) throw Error('policy_mode');
      await capability(authorization,this.env.KEYLESS_DELIVERY_TOKEN);
      const name = await digest('agindow-owner-v1:'+registration);
      if (!this.ctx.id.equals(this.env.AUTHORITY.idFromName(name))) throw Error('owner_scope');
      const now = this.#time();
      const multi='KEYLESS_MULTI_ORG' in this.env&&this.env.KEYLESS_MULTI_ORG==='true';
      const profile=multi?registrationProfile(registration):this.env.KEYLESS_PROFILE;
      if(!profile)throw Error('profile_mode');
      const hostRaw=multi&&'KEYLESS_HOST_POLICY' in this.env?this.env.KEYLESS_HOST_POLICY:undefined;
      if(multi&&typeof hostRaw!=='string')throw Error('profile_mode');
      const next = await accept(this.#projection,JSON.parse(raw),registration,this.env.EXECUTION_ORIGIN,
        profile,this.env.KEYLESS_REVIEW_KEYS,now,hostRaw);
      this.ctx.storage.transactionSync(()=>{
        this.ctx.storage.sql.exec('INSERT INTO owner_policy VALUES(1,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body',JSON.stringify(next));
        this.ctx.storage.sql.exec('UPDATE owner_clock SET floor=? WHERE id=1',now);
      });
      this.#projection = next;
      return `${next.generation}:${next.revision}:${next.sequence}`;
    });
    this.#updates = operation.catch(()=>undefined);
    return operation;
  }
  /** Private readiness capability; never mounted on execution HTTP or consulted by guard. */
  async exchangeReadiness(authorization:string,registration:string):Promise<string> {
    if(!('KEYLESS_READINESS_TOKEN' in this.env)||typeof this.env.KEYLESS_READINESS_TOKEN!=='string')throw Error('unavailable');
    await capability(authorization,this.env.KEYLESS_READINESS_TOKEN);
    const now=this.#time(),p=this.#projection,row=p?.row;
    if(this.env.KEYLESS_ENABLED!=='true'||this.env.KEYLESS_POLICY_MODE!=='auto'||!p||!row||row.id!==registration||
      !this.ctx.id.equals(this.env.AUTHORITY.idFromName(row.owner))||p.status!=='active'||p.notAfter<=now||now<this.#readyAt)throw Error('unavailable');
    const routes=record(JSON.parse(connectionsFor(this.env,row)));
    for(const tool of row.tools){const route=record(routes[row.connection+'/'+tool]);if(route.operation!=='read'||route.tenant!==row.tenant||route.environment!==row.environment)throw Error('unavailable');}
    // A deliberately invalid assertion initializes the real native issuer/config and must reject.
    // It cannot issue a token, consume a valid assertion fence or dispatch upstream.
    const response=await this.#host({issuer:p.origin,notAfter:p.notAfter,registrations:[row]},row,now).fetch(new Request(p.origin+'/v1/token',{
      method:'POST',body:JSON.stringify({grant_type:'urn:ietf:params:oauth:grant-type:token-exchange',subject_token_type:'urn:ietf:params:oauth:token-type:jwt',
        subject_token:'e30.e30.AAAA',registration:row.id,audience:'agindow:guard',tools:row.tools,ttl:60,cnf:{jkt:'A'.repeat(43)}})}));
    if(response.status!==401||this.#projection!==p||this.#time()>=p.notAfter)throw Error('unavailable');
    return JSON.stringify({ready:true,generation:p.generation,revision:p.revision,sequence:p.sequence,checkedAt:now,expiresAt:Math.min(now+30,p.notAfter)});
  }
  async #selection(request: Request, registration: string, now: number): Promise<{config:Policy;row:Registration}> {
    if (this.env.KEYLESS_POLICY_MODE==='auto') {
      const current = this.#projection;
      if (!current) throw Error('policy_unavailable');
      let row = current.row;
      if (new URL(request.url).pathname==='/v1/token') {
        if (current.status!=='active' || current.notAfter<=now || !row || (registration && row.id!==registration)) throw Error('policy_expired');
      } else {
        const authorization = request.headers.get('authorization');
        if (authorization?.startsWith('DPoP ')) {
          let claims:Record<string,unknown>;
          try {claims=jwtPart(authorization.slice(5),1);} catch {claims={};}
          // Native validation rejects malformed credentials; a parse failure must not become SDK fallback.
          row = [current.row,...current.history.filter(h=>h.until>now).map(h=>h.row)].find(r=>
            r && r.tenant===claims.tenant && r.environment===claims.environment && r.connection===claims.connection) ?? row ?? current.history.find(h=>h.until>now)?.row ?? null;
        }
      }
      if (!row) throw Error('validation_unavailable');
      return {config:{issuer:current.origin,notAfter:current.notAfter,registrations:[row]},row};
    }
    if (this.#projection || (this.env.KEYLESS_POLICY_MODE && this.env.KEYLESS_POLICY_MODE!=='manual')) throw Error('policy_mode');
    if(!('KEYLESS_REGISTRATIONS' in this.env)) throw Error('policy_mode');
    const config = await policy(this.env.KEYLESS_REGISTRATIONS,now);
    const row = config.registrations.find(r=>r.id===registration);
    if (!row) throw Error('registration');
    return {config,row};
  }
  protected now(): number {
    return Math.floor(Date.now() / 1000);
  }
  #time(): number {
    const now = this.now();
    if (!Number.isSafeInteger(now) || now < this.#floor) throw Error('clock');
    this.#floor = now;
    return now;
  }
  #host(config: Policy, row: Registration, now: number): NativeHost {
    const bindings: Record<string, unknown> = {
      GUARD_OWNER_ID: row.owner + ':' + this.#activation,
      AGINDOW_LATENCY_CLOCK: 'unavailable',
      GUARD_MACHINE_AUTH: JSON.stringify({
        issuer: config.issuer,
        keys: [],
        keyless_only: [row.tenant],
        legacy_until: 0,
      }),
      GUARD_IDENTITY_EXCHANGE: JSON.stringify({
        issuer: config.issuer,
        kid: 'owner-generated',
        signing_seed: 'owner-generated',
        keys: [
          { issuer: row.id, kid: row.kid, x: row.x, until: Math.min(now + 300, config.notAfter) },
        ],
        registrations: [
          {
            id: row.id,
            kid: row.kid,
            audiences: ['agindow:guard'],
            issuer: row.id,
            subject: row.id,
            audience: row.audience,
            tenant: row.tenant,
            environment: row.environment,
            connection: row.connection,
            tools: row.tools,
            current: true,
            private_key_jwt: true,
          },
        ],
      }),
      GUARD_CONNECTIONS: connectionsFor(this.env,row),
    };
    return new NativeHost(this.ctx, bindings);
  }
  protected admissionCounters(): AdmissionCounters { return { ...this.#counters }; }
  async execute(request: Request, registration: string): Promise<Response> {
    const start = performance.now();
    const response = await this.#execute(request, registration);
    try {
      const outcome = await admission(response);
      this.#counters[outcome]++;
      report(this.#counters, outcome, Math.max(0, performance.now() - start));
    } catch { /* Observability cannot change admission or trigger dispatch. */ }
    return response;
  }
  /** Executes the guard here; the ingress never performs a separate replay-store lookup. */
  async #execute(request: Request, registration: string): Promise<Response> {
    try {
      if (this.env.KEYLESS_ENABLED !== 'true') return json(503, 'identity_unavailable');
      const now = this.#time(), {config,row} = await this.#selection(request,registration,now);
      if (!row || !this.ctx.id.equals(this.env.AUTHORITY.idFromName(row.owner)))
        return json(403, 'identity_scope');
      if (now < this.#readyAt) return json(503, 'identity_warming');
      if (config.issuer !== this.env.EXECUTION_ORIGIN) return json(503, 'identity_unavailable');
      const url = new URL(request.url);
      if (url.origin !== this.env.EXECUTION_ORIGIN) return json(403, 'identity_scope');
      if (url.pathname === '/v1/token') {
        // Exchange-only queue: slow durable admission cannot hold the guard's local admission path.
        const pending = this.#exchanges.then(() => this.#exchange(request, config, row));
        this.#exchanges = pending.catch(() => undefined);
        return await pending;
      }
      return await this.#host(config, row, now).fetch(request);
    } catch {
      return json(503, 'identity_unavailable');
    }
  }
  #current(row:Registration, now:number):boolean {
    const policy=this.#projection;
    return this.env.KEYLESS_POLICY_MODE!=='auto' || (policy?.status==='active' && policy.notAfter>now && JSON.stringify(policy.row)===JSON.stringify(row));
  }
  async #exchange(request: Request, config: Policy, row: Registration): Promise<Response> {
    const now = this.#time();
    if (!this.#current(row,now)) return json(503,'identity_unavailable');
    if (now >= config.notAfter) return json(503, 'identity_unavailable');
    let body: string, input: Record<string, unknown>;
    try {
      body = await readBody(request, 32768);
      input = record(JSON.parse(body));
    } catch {
      return json(400, 'invalid_request');
    }
    if (
      typeof input.ttl !== 'number' ||
      !Number.isSafeInteger(input.ttl) ||
      input.ttl < 1 ||
      input.ttl > 300
    )
      return json(400, 'invalid_request');
    if (input.subject_token_type === 'urn:ietf:params:oauth:token-type:access_token') {
      // A retired parent may finish its lifetime, but cannot issue outside today's reviewed ceiling.
      if(typeof input.subject_token!=='string' || input.connection!==row.connection || !Array.isArray(input.tools) ||
        !input.tools.every((tool:unknown)=>typeof tool==='string' && row.tools.includes(tool))) return json(403,'identity_scope');
      let parent:Record<string,unknown>;
      try {parent=jwtPart(input.subject_token,1);} catch {return json(401,'identity_invalid');}
      if(parent.tenant!==row.tenant || parent.environment!==row.environment || parent.connection!==row.connection)
        return json(403,'identity_scope');
      if(!this.#current(row,this.#time())) return json(503,'identity_unavailable');
      const response=await this.#host(config, row, now).fetch(
        new Request(request.url, { method: 'POST', headers: request.headers, body }),
      );
      return this.#current(row,this.#time()) ? response : json(503,'identity_unavailable');
    }
    if (input.registration !== row.id || typeof input.subject_token !== 'string')
      return json(403, 'identity_registration');
    let assertion: Record<string, unknown>;
    try {
      assertion = jwtPart(input.subject_token, 1);
    } catch {
      return json(400, 'invalid_request');
    }
    const jti = assertion.jti,
      expires = assertion.exp;
    if (
      typeof jti !== 'string' ||
      !jti ||
      jti.length > 256 ||
      typeof expires !== 'number' ||
      !Number.isSafeInteger(expires)
    )
      return json(400, 'invalid_request');
    const id = await digest(row.id + ':' + jti),
      sql = this.ctx.storage.sql;
    // A non-mutating duplicate check precedes crypto; invalid assertions never consume durable state.
    if (
      sql.exec('SELECT id FROM assertion_fence WHERE id=? AND expires>?', id, now).toArray().length
    )
      return json(401, 'identity_replay');
    if (
      sql
        .exec<{ count: number }>(
          'SELECT count(*) AS count FROM assertion_fence WHERE expires>?',
          now,
        )
        .one().count >= 1024
    )
      return json(429, 'identity_capacity');
    const response = await this.#host(config, row, now).fetch(
      new Request(request.url, { method: 'POST', headers: request.headers, body }),
    );
    if (!this.#current(row,this.#time())) return json(503,'identity_unavailable');
    if (!response.ok) return response;
    // Publish only after durable admission. If the commit fails the token is never exposed.
    this.ctx.storage.transactionSync(() => {
      sql.exec('DELETE FROM assertion_fence WHERE expires<=?', now);
      sql.exec('INSERT INTO assertion_fence(id,expires) VALUES (?,?)', id, expires + 30);
      sql.exec('UPDATE owner_clock SET floor=? WHERE id=1', now);
    });
    return response;
  }
}
