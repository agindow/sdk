/** TII-184–190: host-only customer credential ports. Never call an off-path port from a guard. */
export interface KeyContext {
 readonly expectedOrganizationId:string;readonly expectedEnvironment:'test'|'live';readonly requiredScope:string;
}
export interface CustomerGrant {readonly key_id:string;readonly organization_id:string;readonly environment:'test'|'live';readonly scopes:readonly string[]}
export type CustomerResult=CustomerGrant|{readonly refusal:string};
export interface CustomerVerifier {verifyApiKey(presented:unknown,context:KeyContext):Promise<CustomerResult>}
interface VerificationResponse {
 readonly status:number;
 readonly body:{getReader():{read():Promise<{done:boolean;value?:Uint8Array|undefined}>;cancel():Promise<void>;releaseLock():void}}|null;
}
export interface VerificationBinding {
 fetch(url:string,init:{method:string;headers:Record<string,string>;body:string;signal:AbortSignal}):Promise<VerificationResponse>;
}
export interface CustomerKeyBindings {
 CUSTOMER_KEY_ENVIRONMENT?:'test'|'live';
 CUSTOMER_KEY_VERIFIER?:CustomerVerifier;
 CUSTOMER_KEY_SERVICE?:VerificationBinding;
}
export interface AuthRequest {headers:{get(name:string):string|null}}
export type TenantAuthorization={tenant:string}|{status:number;error:'unauthenticated'|'forbidden'|'verifier_unavailable'|'operator_unavailable'};
/** Reserve the v1 public-ID namespace, including malformed secrets; older TI-70 keys remain operator credentials. */
export function isCustomerKey(value:string|null):boolean{return /^Bearer\s+agw_(?:test|live)_[a-f0-9]{32}_/iu.test(value??'');}
export function refusalStatus(reason:string):number {
 return reason==='verifier_unavailable'?503:['wrong_tenant','wrong_environment','insufficient_scope'].includes(reason)?403:401;
}
export async function authorizeTenant(request:AuthRequest,env:CustomerKeyBindings,scope:string,
 legacy:()=>Promise<string|null>):Promise<TenantAuthorization>{
 const presented=request.headers.get('authorization');
 if(!isCustomerKey(presented)){
  try{const tenant=await legacy();return tenant===null?{status:401,error:'unauthenticated'}:{tenant};}
  catch{return {status:503,error:'operator_unavailable'};}
 }
 if(!/^Bearer agw_(test|live)_[a-f0-9]{32}_[a-f0-9]{64}$/u.test(presented??''))return {status:401,error:'unauthenticated'};
 if(!env.CUSTOMER_KEY_VERIFIER&&!env.CUSTOMER_KEY_SERVICE)return {status:401,error:'unauthenticated'};
 const tenant=request.headers.get('x-agindow-tenant');
 if(!tenant||tenant.length>256||/[\u0000-\u0020\u007f]/u.test(tenant))return {status:403,error:'forbidden'};
 if(env.CUSTOMER_KEY_ENVIRONMENT!=='test'&&env.CUSTOMER_KEY_ENVIRONMENT!=='live')return {status:503,error:'verifier_unavailable'};
 const context:KeyContext=Object.freeze({expectedOrganizationId:tenant,expectedEnvironment:env.CUSTOMER_KEY_ENVIRONMENT,requiredScope:scope});
 try{
  const result=env.CUSTOMER_KEY_VERIFIER?await env.CUSTOMER_KEY_VERIFIER.verifyApiKey(presented,context):await verifyBinding(env.CUSTOMER_KEY_SERVICE,presented,context);
  if('refusal' in result){const status=refusalStatus(result.refusal);return {status,error:status===503?'verifier_unavailable':status===403?'forbidden':'unauthenticated'};}
  if(result.organization_id!==tenant||result.environment!==context.expectedEnvironment||!result.scopes.includes(scope))return {status:403,error:'forbidden'};
  return {tenant:result.organization_id};
 }catch{return {status:503,error:'verifier_unavailable'};}
}
async function verifyBinding(binding:VerificationBinding|undefined,presented:string|null,context:KeyContext):Promise<CustomerResult>{
 if(!binding)throw new Error('Verifier unavailable');
 const controller=new AbortController();let timer:ReturnType<typeof setTimeout>|undefined;
 try{return await Promise.race([(async()=>{
  const response=await binding.fetch('https://identity.internal/internal/api-keys/verify',{method:'POST',headers:{authorization:presented??'','content-type':'application/json'},body:JSON.stringify(context),signal:controller.signal});
  if(response.status!==200)return {refusal:response.status===403?'insufficient_scope':response.status===401?'unknown_key':'verifier_unavailable'};
  const value=await readGrant(response);
  if(typeof value!=='object'||value===null||!('key_id' in value)||typeof value.key_id!=='string'||!('organization_id' in value)||typeof value.organization_id!=='string'
   ||!('environment' in value)||(value.environment!=='test'&&value.environment!=='live')||!('scopes' in value)||!Array.isArray(value.scopes)||value.scopes.length>16||value.scopes.some((scope:unknown)=>typeof scope!=='string'))throw new Error('Invalid verifier response');
  return value as CustomerGrant;
 })(),new Promise<never>((_resolve,reject)=>{timer=setTimeout(()=>{controller.abort();reject(new Error('Verifier deadline'));},2000);})]);
 }finally{if(timer!==undefined)clearTimeout(timer);controller.abort();}
}

async function readGrant(response:VerificationResponse):Promise<unknown>{
 const reader=response.body?.getReader();if(!reader)throw new Error('Missing verifier response');
 const chunks:Uint8Array[]=[];let length=0;
 try{
  for(;;){
   const item=await reader.read();if(item.done)break;
   if(!item.value)throw new Error('Incomplete verifier response');
   length+=item.value.length;if(length>8192){await reader.cancel();throw new Error('Verifier response cap');}
   chunks.push(item.value);
  }
  const bytes=new Uint8Array(length);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes)) as unknown;
 }finally{reader.releaseLock();}
}
