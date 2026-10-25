import { expect,it } from 'vitest';
import fixture from '../parity/api-keys.json';
import { validLegacyKey } from '../src/machine-auth.js';
it('accepts the registry v1 test/live presented form without changing fallback or keyless precedence',()=>{
 for(const row of fixture.keys){
  expect(validLegacyKey(row.key)).toBe(true);expect(row.key).toMatch(new RegExp(`^agw_${row.environment}_${row.public_id}_[a-f0-9]{64}$`));
  expect(row.key.slice(-4)).toBe(row.last4);expect(Buffer.byteLength(`Bearer ${row.key}`)).toBeLessThanOrEqual(1024);
 }
});
