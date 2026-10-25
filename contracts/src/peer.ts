import {microUnits} from './investigation.js';
import type {PeerObservation} from './generated/peer_observation.js';
export function exactI64(value:string):bigint {
 if(!/^(0|-[1-9][0-9]{0,18}|[1-9][0-9]{0,18})$/u.test(value))throw new Error('Unsupported exact scalar');
 const result=BigInt(value);if(result< -9223372036854775808n||result>9223372036854775807n)throw new Error('Unsupported exact scalar');return result;
}
export function checkPeerObservation(value:PeerObservation):void {
 if(microUnits(value.sequence)===0n||microUnits(value.contract_version)===0n)throw new Error('Invalid peer sequence/version');
 const at=Date.parse(value.observed_at),expires=Date.parse(value.expires_at);
 if(!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$/u.test(value.observed_at)||!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$/u.test(value.expires_at)||expires<=at||expires-at>600000)throw new Error('Invalid peer lifetime');
 if(value.status==='available') {if(value.value===null||value.reason!=='')throw new Error('Invalid available peer');exactI64(value.value);}
 else if(value.value!==null||value.reason==='')throw new Error('Unavailable peer must invalidate');
 const url=new URL(value.source_url);
 if(url.protocol!=='https:'||url.username||url.password||url.hash)throw new Error('Invalid peer authority');
 for(const key of url.searchParams.keys())if(/key|token|secret|password|credential|authorization/iu.test(key))throw new Error('Credential-bearing peer URL');
}
