export interface OrgBinding {tenant:string;profile:string;generation:string;nonce:string}
import { initial, type ControlState } from './state.js';
/** SQL is confined to control-plane events and execution-owner initialization/update. */
export class ControlStore {
  constructor(readonly storage: DurableObjectStorage, now: number) {
    storage.sql.exec('CREATE TABLE IF NOT EXISTS registration_control (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)');
    storage.sql.exec('CREATE TABLE IF NOT EXISTS registration_commands (id TEXT PRIMARY KEY, digest TEXT NOT NULL, expires INTEGER NOT NULL, receipt TEXT NOT NULL)');
    storage.sql.exec('INSERT OR IGNORE INTO registration_control VALUES(1,?)',JSON.stringify(initial(crypto.randomUUID(),now)));
  }
  orgBinding():OrgBinding|null {
    this.storage.sql.exec('CREATE TABLE IF NOT EXISTS org_binding(id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)');
    const row=this.storage.sql.exec<{body:string}>('SELECT body FROM org_binding WHERE id=1').toArray()[0];
    return row?JSON.parse(row.body) as OrgBinding:null;
  }
  bindOrg(tenant:string,profile:string,nonce:string):boolean {
    const old=this.orgBinding(),state=this.read();
    if(old){if(old.tenant!==tenant||old.profile!==profile||old.generation!==state.generation||old.nonce!==nonce)throw Error('org_binding');return false;}
    if(state.snapshot!==null||state.revision!==0)throw Error('org_binding');
    this.storage.sql.exec('INSERT INTO org_binding VALUES(1,?)',JSON.stringify({tenant,profile,generation:state.generation,nonce}));return true;
  }
  read(): ControlState {
    const body = this.storage.sql.exec<{body:string}>('SELECT body FROM registration_control WHERE id=1').one().body;
    // The private versioned SQL record is the single persistence boundary; digest checks still precede renewal.
    const state = JSON.parse(body) as ControlState;
    if (!state || typeof state.generation !== 'string' || !Number.isSafeInteger(state.revision) ||
      !Number.isSafeInteger(state.floor) || !Array.isArray(state.pending) || state.pending.length>256 || !Array.isArray(state.retry) || state.retry.length>256) throw Error('authority_unavailable');
    return state;
  }
  write(state: ControlState): void {
    this.storage.sql.exec('UPDATE registration_control SET body=? WHERE id=1',JSON.stringify(state));
  }
  receipt(id: string, digest: string, now: number): string | undefined {
    const row = this.storage.sql.exec<{digest:string;receipt:string}>('SELECT digest,receipt FROM registration_commands WHERE id=? AND expires>?',id,now).toArray()[0];
    if (row && row.digest !== digest) throw Error('command_replay');
    return row?.receipt;
  }
  commit(state: ControlState, id: string, digest: string, until: number, receipt: string, now: number): void {
    this.storage.transactionSync(()=>{
      this.storage.sql.exec('DELETE FROM registration_commands WHERE expires<=?',now);
      if (this.storage.sql.exec<{n:number}>('SELECT count(*) AS n FROM registration_commands').one().n>=1024) throw Error('capacity');
      this.storage.sql.exec('INSERT INTO registration_commands VALUES(?,?,?,?)',id,digest,until,receipt);
      this.write(state);
    });
  }
}
