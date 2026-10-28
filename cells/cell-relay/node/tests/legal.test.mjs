import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { stageLegalFiles } from '../scripts/legal.mjs';

test('repeated package staging uses the public runtime NOTICE and preserves the private root', () => {
  const root = mkdtempSync(resolve(tmpdir(), 'agindow-notice-'));
  const project = resolve(root, 'cells/cell-relay/node');
  try {
    mkdirSync(project, { recursive: true });
    writeFileSync(resolve(root, 'LICENSE'), 'Apache license fixture');
    writeFileSync(resolve(root, 'NOTICE'), 'Private repository scope fixture');
    writeFileSync(resolve(root, 'cells/cell-relay/NOTICE'), 'Public runtime scope fixture');
    for (let count = 0; count < 2; count++) {
      writeFileSync(resolve(project, 'NOTICE'), 'Stale package notice');
      stageLegalFiles(root, project);
      assert.equal(readFileSync(resolve(project, 'NOTICE'), 'utf8'), 'Public runtime scope fixture');
      assert.equal(readFileSync(resolve(project, 'LICENSE'), 'utf8'), 'Apache license fixture');
      assert.equal(readFileSync(resolve(root, 'NOTICE'), 'utf8'), 'Private repository scope fixture');
    }
    unlinkSync(resolve(root, 'cells/cell-relay/NOTICE'));
    assert.throws(() => stageLegalFiles(root, project), /ENOENT/);
    assert.equal(readFileSync(resolve(project, 'NOTICE'), 'utf8'), 'Public runtime scope fixture');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
