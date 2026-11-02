import { expect, it } from 'vitest';
import { selectiveEngine, repairOptions } from '../src/selective.js';
import type { RenameRule } from '../src/selective.js';
import type { Engine, Verdict } from '../src/types.js';
const pass: Verdict = {halt:false, bypassed:false, class:null, findings:0, detectors:0, malformed:false};
const base: Engine = {begin: () => ({request:pass, finish:() => pass, close() {}})};
const rule: RenameRule = {tool:'nested', side:'request', from:'old', to:'new', migration:'r1', confidence:1,
  parents:['/items/0'], contract:'c1', provenance:'owner-review', expiresAt:4102444800000};
it('independently rejects candidate changes to values, siblings, unselected elements and whitespace', () => {
  for (const body of [
    '{"items":[{"new":2},{"old":2}],"keep":7}',
    '{"items":[{"new":1},{"old":2}],"keep":8}',
    '{"items":[{"new":1},{"new":2}],"keep":7}',
    ' {"items":[{"new":1},{"old":2}],"keep":7}',
  ]) {
    let verified = 0;
    const engine = selectiveEngine(base, {rules:[rule],verify:() => {verified++; return true;}},
      () => JSON.stringify({matched:true,applied:true,halt:false,body}));
    expect(() => engine.begin({items:[{old:1},{old:2}],keep:7},'repair','read',true,'nested')).toThrow('preservation');
    expect(verified).toBe(0);
  }
});
it('copies and freezes reviewed scope facts so caller mutation cannot expand authority', () => {
  const parents = ['/items/0'];
  const options = repairOptions({rules:[{...rule,parents}],verify:() => true},false);
  parents.push('/items/1');
  const selected = options?.rules[0];
  expect(selected && !('recipe' in selected) ? selected.parents : undefined).toEqual(['/items/0']);
  expect(Object.isFrozen(options?.rules[0])).toBe(true);
});
it('refuses duplicate and ancestor source selections before execution', () => {
  for (const parents of [['/items/0','/items/0'],['','/old']]) {
    expect(() => repairOptions({rules:[{...rule,parents}],verify:() => true},false)).toThrow('invalid selectiveRepair');
  }
});
it('rejects unpaired surrogate parent pointers as named setup errors', () => {
  expect(() => repairOptions({rules:[{...rule,parents:['/\uD800']}],verify:() => true},false)).toThrow('invalid selectiveRepair');
});
