import { record } from '../policy.js';
import { exact, integer, text } from './protocol.js';
export interface PublicJwk {kty:'OKP';crv:'Ed25519';kid:string;x:string}
export type Intent={operation:'create';connection:string;jwk:PublicJwk}|{operation:'rotate';id:string;revision:number;jwk:PublicJwk}|{operation:'revoke';id:string;revision:number};
export interface IdentityIntent {actor:string;tenant:string;environment:'test';role:'owner';authenticatedAt:number;authorizedAt:number}
export function publicJwk(input:unknown):PublicJwk {
 const j=record(input);exact(j,['kty','crv','kid','x']);
 const kid=text(j.kid),x=text(j.x);
 if(j.kty!=='OKP'||j.crv!=='Ed25519'||/[/*]/u.test(kid)||!/^[-_A-Za-z0-9]{43}$/u.test(x))throw Error('invalid_public_key');
 const decoded=atob(x.replace(/-/gu,'+').replace(/_/gu,'/'));
 if(decoded.length!==32||btoa(decoded).replace(/=/gu,'').replace(/\+/gu,'-').replace(/\//gu,'_')!==x)throw Error('invalid_public_key');
 return {kty:'OKP',crv:'Ed25519',kid,x};
}
export function intent(input:unknown):Intent {
 const b=record(input);
 if(b.operation==='create'){exact(b,['operation','connection','jwk']);return {operation:'create',connection:text(b.connection),jwk:publicJwk(b.jwk)};}
 if(b.operation==='rotate'){exact(b,['operation','id','revision','jwk']);return {operation:'rotate',id:text(b.id),revision:integer(b.revision,1),jwk:publicJwk(b.jwk)};}
 if(b.operation==='revoke'){exact(b,['operation','id','revision']);return {operation:'revoke',id:text(b.id),revision:integer(b.revision,1)};}
 throw Error('invalid_intent');
}
export function identity(input:unknown,now:number,recent=true):IdentityIntent {
 const a=record(input);exact(a,['actor','tenant','environment','role','authenticatedAt','authorizedAt']);
 const authenticatedAt=integer(a.authenticatedAt),authorizedAt=integer(a.authorizedAt);
 if(a.role!=='owner'||a.environment!=='test'||authorizedAt>now||now-authorizedAt>30||authenticatedAt>now||recent&&now-authenticatedAt>300)throw Error('forbidden');
 return {actor:text(a.actor),tenant:text(a.tenant),environment:'test',role:'owner',authenticatedAt,authorizedAt};
}
