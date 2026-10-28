import { admissionMutation } from './admission-mutation.mjs';
import { build } from 'esbuild';
import { renewalMutation } from './renewal-mutation.mjs';
/** @param {string} name @returns {Promise<unknown>} */
export async function loadRenewal(name) {
  const result = await build({ entryPoints:[new URL(`../../owner/control/${name}.ts`,import.meta.url).pathname], plugins:[...admissionMutation(process.env['AGINDOW_ADMISSION_MUTATION']),...renewalMutation(process.env['AGINDOW_RENEWAL_MUTATION'])], bundle:true, write:false, platform:'node', format:'esm', target:'es2022', logLevel:'silent' });
  const file = result.outputFiles?.[0];
  if (file === undefined) throw Error('missing compiled test module');
  return import('data:text/javascript;base64,' + Buffer.from(file.text).toString('base64'));
}
