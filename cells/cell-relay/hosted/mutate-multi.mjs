import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const cwd=fileURLToPath(new URL('../',import.meta.url));
for(const [mutation,file,pattern] of [
 ['orgNamespace','runtime','private input'],['orgRows','policy','org profiles'],
 ['orgHint','policy','org profiles'],['orgNonce','runtime','directory loss'],
 ['catalog','policy','host read catalog'],['batch','boundaries','bounded batches'],
]){
 if(!mutation||!file||!pattern)throw Error('mutation fixture');
 const run=spawnSync(process.execPath,['--test',`--test-name-pattern=${pattern}`,`hosted/tests/multi-${file}.test.mjs`],{cwd,encoding:'utf8',env:{...process.env,AGINDOW_ADMISSION_MUTATION:mutation}});
 if(run.error||run.signal||run.status!==1||!run.stdout.includes('ERR_ASSERTION')||run.stdout.includes('mutation target changed'))throw Error(`multi-org mutation ${mutation} did not fail behavior: ${run.stdout.slice(-1500)} ${run.stderr.slice(-500)}`);
 console.log(`multi-org mutation rejected: ${mutation}`);
}
