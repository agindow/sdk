import { copyFileSync } from 'node:fs';
import { resolve } from 'node:path';
/** @param {string} root @param {string} project */
export function stageLegalFiles(root, project) {
  // The monorepo NOTICE describes private components absent from SDK distributions.
  copyFileSync(resolve(root, 'cells/cell-relay/NOTICE'), resolve(project, 'NOTICE'));
  copyFileSync(resolve(root, 'LICENSE'), resolve(project, 'LICENSE'));
}
