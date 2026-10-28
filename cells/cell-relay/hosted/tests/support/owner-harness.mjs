import { admissionMutation } from './admission-mutation.mjs';
import { generateKeyPairSync, sign, createHash } from 'node:crypto';
import { mkdtemp, rm, readdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { renewalMutation } from './renewal-mutation.mjs';
import { loadRenewal } from './load-renewal.mjs';
import { ownerMutation } from './owner-mutation.mjs';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare-owner';
import { createProofSigner } from '../../../node/dist/esm/index.js';
const initial = 1_800_000_000;
/** @param {object} value */
const b64 = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
/** @param {import('node:crypto').KeyObject} key @param {number} now @param {string} id */
function assertion(key, now, id, registration = 'workload', lifetime = 120) {
  const input =
    b64({ alg: 'EdDSA', typ: 'JWT', kid: 'client-key' }) +
    '.' +
    b64({
      iss: registration,
      sub: registration,
      aud: 'https://guard.invalid/v1/token',
      iat: now,
      nbf: now,
      exp: now + lifetime,
      jti: id,
    });
  return input + '.' + sign(null, Buffer.from(input), key).toString('base64url');
}

/** @param {{auto?:boolean,now?:number,multi?:boolean}} [settings] */
export async function ownerHarness(settings = {}) {
  const initial=settings.now??1_800_000_000;
  const dir = await mkdtemp(join(tmpdir(), 'agindow-owner-')),
    client = generateKeyPairSync('ed25519'), reviewer = generateKeyPairSync('ed25519'), nextReviewer=generateKeyPairSync('ed25519');
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  const outfile = join(dir, 'worker.mjs');
  await writeFile(
    join(dir, 'native-copy.js'),
    await readFile(join(root, 'crates/http-host/build/index.js')),
  );
  await build({
    entryPoints: [fileURLToPath(new URL('./owner-worker.ts', import.meta.url))],
    outfile,
    bundle: true,
    platform: 'browser',
    target: 'es2022',
    format: 'esm',
    define: { FIXTURE_CLOCK_START: 'globalThis.__fixtureClockStart' },
    banner: { js: 'globalThis.__fixtureClockStart=1800000000;' },
    external: ['cloudflare:workers', 'node:*'],
    plugins: [
      ...admissionMutation(process.env['AGINDOW_ADMISSION_MUTATION']),
      ...renewalMutation(process.env['AGINDOW_RENEWAL_MUTATION']),
      ...ownerMutation(process.env['AGINDOW_OWNER_MUTATION']),
      {
        name: 'native-wasm',
        setup(plugin) {
          plugin.onResolve({ filter: /[.]wasm$/ }, () => ({
            path: './native.wasm',
            external: true,
          }));
        },
      },
    ],
    alias: {
      '@agindow/native-host': fileURLToPath(new URL('./native-switch.ts', import.meta.url)),
      '@agindow/native-one': join(root, 'crates/http-host/build/index.js'),
      '@agindow/native-two': join(dir, 'native-copy.js'),
    },
    logLevel: 'silent',
  });
  await writeFile(
    join(dir, 'native.wasm'),
    await readFile(join(root, 'crates/http-host/build/index_bg.wasm')),
  );
  /** @type {import('miniflare-owner').V4ModuleDefinition[]} */
  const modules = [
    { type: 'ESModule', path: 'worker.mjs', contents: await readFile(outfile, 'utf8') },
    ...(await Promise.all(
      (await readdir(dir))
        .filter((name) => name.endsWith('.wasm'))
        .map(
          /** @returns {Promise<import('miniflare-owner').V4ModuleDefinition>} */ async (name) => ({
            type: 'CompiledWasm',
            path: name,
            contents: await readFile(join(dir, name)),
          }),
        ),
    )),
  ];
  const registrations = JSON.stringify({
    issuer: 'https://guard.invalid',
    not_after: initial + 3600,
    registrations: [
      {
        id: 'foreign',
        kid: 'client-key',
        x: client.publicKey.export({ format: 'jwk' }).x,
        audience: 'https://guard.invalid/v1/token',
        tenant: 'b',
        environment: 'test',
        connection: 'd',
        tools: ['op'],
      },
      {
        id: 'workload',
        kid: 'client-key',
        x: client.publicKey.export({ format: 'jwk' }).x,
        audience: 'https://guard.invalid/v1/token',
        tenant: 'a',
        environment: 'test',
        connection: 'c',
        tools: ['op', 'recovery'],
      },
    ],
  });
  const bindings = {
    KEYLESS_POLICY_MODE: settings.auto ? 'auto' : 'manual',
    KEYLESS_PROFILE: 'primary',
    KEYLESS_CONTROL_ENABLED: 'true',
    KEYLESS_RENEWAL_SECONDS:'600',
    KEYLESS_DELIVERY_TOKEN: 'fixture-delivery-capability-'.padEnd(43,'x'),
    KEYLESS_REVIEW_KEYS: JSON.stringify([{kid:'reviewer',x:reviewer.publicKey.export({format:'jwk'}).x,until:initial+2592000}]),
    EXECUTION_ORIGIN: 'https://guard.invalid',
    KEYLESS_ENABLED: 'true',
    KEYLESS_READINESS_TOKEN:'fixture-readiness-capability-'.padEnd(43,'x'),
    ADMISSION_READINESS_TOKEN:'fixture-readiness-capability-'.padEnd(43,'x'),
    FEATURE_FLAGS:'workload-self-serve',ADMISSION_POLICY:'',ADMISSION_REVIEW_KID:'reviewer-next',
    ADMISSION_SIGNING_JWK:JSON.stringify(nextReviewer.privateKey.export({format:'jwk'})),
    KEYLESS_REGISTRATIONS: registrations,
    GUARD_CONNECTIONS: JSON.stringify({
      'c/recovery': {
        tenant: 'a',
        environment: 'test',
        url: 'https://vendor.invalid/write',
        method: 'POST',
        operation: 'write',
        selection: {
          tenant: 'a',
          connection: 'c',
          vendor: 'v',
          operation: 'recovery',
          segment: 's',
          fields: [
            {
              side: 'response',
              path: '/value',
              baseline: Array.from({ length: 128 }, () => '{"value":1}'),
              live: Array.from({ length: 128 }, () => '{"value":100}'),
              enumeration: true,
            },
          ],
        },
      },
      'd/op': { tenant: 'b', environment: 'test', url: 'https://vendor.invalid/read', method: 'POST', operation: 'read' },
      'c/op': {
        tenant: 'a',
        environment: 'test',
        url: 'https://vendor.invalid/read',
        method: 'POST',
        operation: 'read',
      },
    }),
  };
  const hostPolicy={format:2,version:'host-v2',reviewerKid:'reviewer-next',origin:'https://guard.invalid',profileKey:'customer',environment:'test',connection:'c',tools:['op'],audience:'https://guard.invalid/v1/token',until:initial+86400};
  const hostRaw=JSON.stringify(hostPolicy);
  if(settings.multi){
    Object.assign(bindings,{KEYLESS_MULTI_ORG:'true',KEYLESS_HOST_POLICY:hostRaw,ADMISSION_POLICY:hostRaw,
      KEYLESS_READ_CATALOG:JSON.stringify({'c/op':{url:'https://vendor.invalid/read',method:'POST',operation:'read'}})});
    bindings.KEYLESS_REVIEW_KEYS=JSON.stringify([...JSON.parse(bindings.KEYLESS_REVIEW_KEYS),{kid:'reviewer-next',x:nextReviewer.publicKey.export({format:'jwk'}).x,until:hostPolicy.until,policy:hostPolicy}]);
  }
  const naming=/** @type {typeof import('../../owner/control/host-policy.js')} */(await loadRenewal('host-policy'));
  const profile=async(tenant='a')=>settings.multi?naming.orgProfile(naming.hostPolicy(hostPolicy),tenant):'primary';
  let dispatches = 0;
  /** @param {number} time @returns {import('miniflare-owner').V4MiniflareOptions} */
  const optionsFor = (time) => ({
    resourcePersistencePath: join(dir, 'storage'),
    workers: ['authority', 'ingress-a', 'ingress-b','control','renewal-job','admission','customer'].map((name) => ({
      name,
      modules: modules.map((module) =>
        module.type === 'ESModule' && typeof module.contents === 'string'
          ? {
              ...module,
              contents: module.contents.replace(
                'globalThis.__fixtureClockStart=1800000000;',
                `globalThis.__fixtureClockStart=${time};`,
              ),
            }
          : module,
      ),
      compatibilityDate: '2026-10-26',
      compatibilityFlags: ['nodejs_compat'],
      bindings: name==='renewal-job' ? {KEYLESS_RENEWAL_ENABLED:'true',FIXTURE_ROLE:'job'} : {...bindings,FIXTURE_ROLE:name==='customer'?'customer':name==='control' ? 'control' : 'ingress'},
      serviceBindings: name==='customer'?{CUSTOMER_WORKLOADS:{name:'admission',entrypoint:'CustomerWorkloads'}}:name==='renewal-job' ? {RENEWAL:{name:'control',entrypoint:settings.multi?'MultiRenewal':'Renewal'}} : name==='admission' ? {ADMINISTRATION:{name:'control',entrypoint:settings.multi?'MultiAdministration':'Administration'}} : {},
      durableObjects: name==='renewal-job' ? {} : {
        ORG_DIRECTORY:{className:'FixtureDirectory',scriptName:'control',useSQLite:true},
        ADMISSIONS:{className:'FixtureAdmission',scriptName:'admission',useSQLite:true},
        AUTHORITY: { className: 'FixtureOwner', scriptName: 'authority', useSQLite: true },
        EXECUTORS: { className: 'FixtureOwner', scriptName: 'authority', useSQLite: true },
        POLICIES: { className: 'FixturePolicy', scriptName: 'control', useSQLite: true },
      },
      outboundService: async (request) => {
        dispatches++;
        return request.url.endsWith('/write')
          ? new Response(' {"value":100}\n', { status: 201, headers: { 'content-length': '15' } })
          : new Response(' {"owner":true}\n');
      },
    })),
  });
  let runtime = new Miniflare(convertV4MiniflareOptions(optionsFor(initial)));
  let now = initial;
  const ownerName = createHash('sha256').update('agindow-owner-v1:workload').digest('hex');
  /** @typedef {{setTime:(value:number)=>Promise<void>,resetVerifier:()=>Promise<void>,eraseLedger:()=>Promise<void>,fillLedger:(until:number)=>Promise<void>,retained:()=>Promise<number>,partition:(value:boolean)=>Promise<void>,execute:(request:Request,registration:string)=>Promise<Response>,counters:()=>Promise<string>,configure:(raw:string)=>Promise<void>,installPolicy:(raw:string,cap:string,id:string)=>Promise<string>,setMode:(mode:string)=>Promise<void>,configureKeys:(raw:string)=>Promise<void>,setEnabled:(value:string)=>Promise<void>,deliveryDown:(value:boolean)=>Promise<void>,eraseGuardSql:()=>Promise<void>}} FixturePort */
  /** @param {ReturnType<Awaited<ReturnType<Miniflare['getDurableObjectNamespace']>>['get']>} stub */
  const fixturePort = (stub) => /** @type {typeof stub & FixturePort} */ (stub);
  const owner = async (registration = 'workload') => {
    const namespace = await runtime.getDurableObjectNamespace('AUTHORITY', 'authority');
    return fixturePort(namespace.get(namespace.idFromName(createHash('sha256').update('agindow-owner-v1:' + registration).digest('hex'))));
  };
  const ingress = async (name = 'ingress-a') => runtime.getWorker(name);
  const signer = createProofSigner();
  /** @param {string} id @param {string} [name] @param {string[]} [tools] */
  const exchange = async (id, name = 'ingress-a', tools = ['op'], registration = 'workload', key = client.privateKey, issuedAt = now, lifetime = 120) => {
    const body = {
      grant_type: 'urn:ietf:params:oauth:grant-type:token-exchange',
      subject_token_type: 'urn:ietf:params:oauth:token-type:jwt',
      subject_token: assertion(key, issuedAt, id, registration, lifetime),
      registration,
      audience: 'agindow:guard',
      tools,
      ttl: 300,
      cnf: { jkt: signer.thumbprint },
    };
    return (await ingress(name)).fetch('https://guard.invalid/v1/token', {
      method: 'POST',
      body: JSON.stringify(body),
    });
  };
  /** @param {string} access @param {string} proof @param {string} name */
  const guard = async (access, proof, name, connection = 'c') =>
    (await ingress(name)).fetch('https://guard.invalid/v1/guard', {
      method: 'POST',
      headers: { authorization: `DPoP ${access}`, dpop: proof },
      body: JSON.stringify({
        tool: 'op',
        connection,
        operation: 'read',
        args: {},
        mode: 'observe',
      }),
    });
  /** @typedef {{advanceAfterCommit:(raw:string)=>Promise<void>,competeOnce:(raw:string)=>Promise<void>,loseNextAck:()=>Promise<void>,activity:()=>Promise<number>,inspect:()=>Promise<string>,submit:(raw:string)=>Promise<string>,renewCurrent:()=>Promise<string>,deliver:()=>Promise<void>,eraseControl:()=>Promise<void>,corruptRows:()=>Promise<void>,configureKeys:(raw:string)=>Promise<void>,restoreControl:(raw:string)=>Promise<void>,setTime:(value:number)=>Promise<void>,alarmAt:()=>Promise<number|null>,runAlarm:()=>Promise<void>,fillCommands:(until:number)=>Promise<void>}} ControlPort */
  const control = async (tenant='a') => {
    const ns = await runtime.getDurableObjectNamespace('POLICIES','control');
    return /** @type {ReturnType<typeof ns.get> & ControlPort} */ (ns.get(ns.idFromName(await profile(tenant))));
  };
  /** @param {Partial<import('../../owner/control/protocol.js').Command>} [overrides] */
  const command = async (overrides = {},tenant='a') => {
    const state = JSON.parse(await (await control(tenant)).inspect());
    const protocol = /** @type {typeof import('../../owner/control/protocol.js')} */ (await loadRenewal('protocol'));
    const content = await protocol.reviewed('https://guard.invalid',await profile(tenant),overrides.rows ?? JSON.parse(registrations).registrations);
    const payload = {version:/** @type {const} */ (1),origin:'https://guard.invalid',profile:await profile(tenant),generation:state.generation,
      previousGeneration:null,expectedRevision:state.revision,id:crypto.randomUUID(),kid:'reviewer',iat:now,exp:now+120,
      action:/** @type {const} */ ('admit'),reviewUntil:initial+86400,...content,...overrides};
    return JSON.stringify({payload,signature:sign(null,Buffer.from(protocol.encoded(payload)),payload.kid==='reviewer-next' ? nextReviewer.privateKey : reviewer.privateKey).toString('base64url')});
  };
  /** @typedef {{list:(identity:string)=>Promise<string>,mutate:(identity:string,id:string,input:string)=>Promise<string>,setFeatureEnabled:(enabled:boolean)=>Promise<void>,setSigningEnabled:(enabled:boolean)=>Promise<void>,configurePolicy:(raw:string)=>Promise<void>,configureCatalog:(raw:string)=>Promise<void>,setTime:(now:number)=>Promise<void>,stored:()=>Promise<string>,runAlarm:()=>Promise<void>,eraseAdmission:()=>Promise<void>}} AdmissionPort */
  const admission=async(tenant='a')=>{const ns=await runtime.getDurableObjectNamespace('ADMISSIONS','admission');return /** @type {ReturnType<typeof ns.get> & AdmissionPort} */(ns.get(ns.idFromName(await profile(tenant))));};
  const setupAdmission=async(tenant='a')=>{
    const state=JSON.parse(await (await control()).inspect());
    const p={version:'p1',tenant,environment:'test',origin:'https://guard.invalid',profile:'primary',connection:'c',audience:'https://guard.invalid/v1/token',tools:['op'],until:initial+86400,bootstrapGeneration:state.generation};
    const keys=JSON.stringify([JSON.parse(bindings.KEYLESS_REVIEW_KEYS)[0],{kid:'reviewer-next',x:nextReviewer.publicKey.export({format:'jwk'}).x,until:p.until,policy:p}]);
    bindings.KEYLESS_REVIEW_KEYS=keys;bindings.ADMISSION_POLICY=JSON.stringify(p);
    const routes=JSON.parse(bindings.GUARD_CONNECTIONS);for(const row of Object.values(routes))if(row.tenant==='a')row.tenant=tenant;bindings.GUARD_CONNECTIONS=JSON.stringify(routes);
    await (await control()).configureKeys(keys);await (await admission()).configurePolicy(JSON.stringify(p));
    await runtime.dispose();runtime=new Miniflare(convertV4MiniflareOptions(optionsFor(now)));
    return p;
  };
  const directory=async()=>{const ns=await runtime.getDurableObjectNamespace('ORG_DIRECTORY','control');return /** @type {ReturnType<typeof ns.get> & {enrolled:()=>Promise<number>,erase:()=>Promise<void>,runAlarm:()=>Promise<void>}} */(ns.get(ns.idFromName('host-directory')));};
  const customer={
    /** @param {string} identity */
    list:async(identity)=>{const r=await(await ingress('customer')).fetch('https://private.invalid',{method:'POST',body:JSON.stringify({identity})});if(!r.ok)throw Error('refused');return r.text();},
    /** @param {string} identity @param {string} id @param {string} input */
    mutate:async(identity,id,input)=>{const r=await(await ingress('customer')).fetch('https://private.invalid',{method:'POST',body:JSON.stringify({identity,id,input})});if(!r.ok)throw Error('refused');return r.text();},
  };
  return {customer, enrolled:async()=>(await directory()).enrolled(),eraseDirectory:async()=>(await directory()).erase(),directory,admission,setupAdmission,client, signer, ownerName, owner, ingress, exchange, guard, registrations, control, command,
    deliveryCapability:bindings.KEYLESS_DELIVERY_TOKEN,
    reviewKeys:JSON.parse(bindings.KEYLESS_REVIEW_KEYS),
    nextReviewKey:{kid:'reviewer-next',x:nextReviewer.publicKey.export({format:'jwk'}).x,until:initial+2592000},
    get now() { return now; }, get dispatches() { return dispatches; },
    /** @param {number} value */
    async setTime(value) { now = value; await (await owner()).setTime(now); if(settings.auto) await (await control()).setTime(now);if(settings.multi)await(await admission()).setTime(now); },
    async restart() { await runtime.dispose(); runtime = new Miniflare(convertV4MiniflareOptions(optionsFor(now))); },
    async close() { await runtime.dispose(); await rm(dir, { recursive: true, force: true }); },
    namespace: () => runtime.getDurableObjectNamespace('AUTHORITY', 'authority'),
    fixturePort,
  };
}

/** @param {{ json(): Promise<unknown> }} response */
export async function credential(response) {
  const value = await response.json();
  if (typeof value !== 'object' || value === null || !('access_token' in value) ||
    typeof value.access_token !== 'string' || !('expires_at' in value) || typeof value.expires_at !== 'number')
    throw Error('invalid fixture credential');
  return { access_token: value.access_token, expires_at: value.expires_at };
}
