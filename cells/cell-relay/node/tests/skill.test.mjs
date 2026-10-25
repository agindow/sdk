import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const skill = readFileSync(new URL('../skills/guarding-nango-functions/SKILL.md', import.meta.url), 'utf8');
test('Nango skill has portable frontmatter and delegates unchanged runtime generation', () => {
  const frontmatter = /^---\nname: ([a-z0-9-]+)\ndescription: ([^\n]+)\n---\n/.exec(skill);
  assert.ok(frontmatter); assert.equal(frontmatter[1], 'guarding-nango-functions');
  assert.ok((frontmatter[2]?.length ?? 0) > 40);
  for (const command of ['agindow report <project>', 'agindow scan <project> --snapshot <local-contracts.json>', 'agindow codegen <local-contracts.json>']) assert.ok(skill.includes(command));
  assert.match(skill, /Use the generator's output unchanged/);
  assert.match(skill, /Do not reproduce or hand-edit its runtime/);
  assert.match(skill, /request check must precede its provider call/);
  assert.match(skill, /second run must report zero changed files/);
  assert.match(skill, /do not invent its result or contact a\nprovider/);
  assert.doesNotMatch(skill, /```(?:ts|typescript|javascript)/);
});
