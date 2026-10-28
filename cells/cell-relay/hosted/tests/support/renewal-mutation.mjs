import { readFile } from 'node:fs/promises';
/** @param {string|undefined} selected @returns {import('esbuild').Plugin[]} */
export function renewalMutation(selected) {
  if(selected===undefined) return [];
  /** @type {Record<string,readonly [string,string,string]>} */
  const cases={
    digest:['protocol','if ((await reviewed(origin,profile,state.rows)).digest !== state.digest)','if (false)'],
    revision:['state','c.expectedRevision !== state.revision','false'],
    status:['protocol',"state.status !== 'active'",'false'],
    expiry:['protocol','now >= state.notAfter','false'],
    generation:['delivery','c.previousGeneration!==current.generation','false'],
    hotpath:['../owner',"if (!current) throw Error('policy_unavailable');","if (!current) throw Error('policy_unavailable'); this.ctx.storage.sql.exec('SELECT body FROM owner_policy WHERE id=1');"],
    cadence:['delivery-plan','ready.length ? 1 :','ready.length ? 60 :'],
    priority:['delivery-plan','...pending.filter(id=>ids.includes(id)),...ids','...ids,...pending.filter(id=>ids.includes(id))'],
    alarm:['authority','try { await this.deliver(); }','try { await this.renewCurrent(); }'],
  };
  const entry=cases[selected]; if(!entry) throw Error('unknown renewal mutation');
  const target=entry[0]==='../owner' ? '/owner/owner.ts' : `/control/${entry[0]}.ts`;
  return [{name:'renewal-mutation',setup(plugin){plugin.onLoad({filter:/\.ts$/},async({path})=>{
    if(!path.endsWith(target)) return undefined;
    const source=await readFile(path,'utf8');
    if(source.split(entry[1]).length!==2) throw Error('mutation target changed');
    return {contents:source.replace(entry[1],entry[2]),loader:'ts'};
  });}}];
}
