import { sourceFixCommand } from './source-fixes/command.js';
import { migrationCommand } from './migration/command.js';
import { bindingCommand } from './binding/command.js';
import { rotationCommand } from './rotation/command.js';
import { report } from './report/index.js';
import { readContracts, generateGuard, patchProject } from './codegen.js';
declare const __AGINDOW_BANNER__: string;
async function main(args: readonly string[]): Promise<number> {
  if (args[0] === 'export-source-fix') return sourceFixCommand(args.slice(1));
  if (args[0] === 'generate-source-patch') return migrationCommand(args.slice(1));
  if (args[0] === 'bind-source' || args[0] === 'resolve-source') return bindingCommand(args[0], args.slice(1));
  if (args[0] === "rotation-readiness") return rotationCommand(args.slice(1));
  const [command, directory, ...flags] = args;
  if (command === 'codegen' && directory !== undefined && flags.length === 0) {
    for (const contract of readContracts(directory, process.env['AGINDOW_DISABLED'] === 'true')) console.log(generateGuard(contract));
    return 0;
  }
  if (command === 'scan' && directory !== undefined && flags.length === 2 && flags[0] === '--snapshot' && flags[1] !== undefined) {
    const result = patchProject(directory, readContracts(flags[1], process.env['AGINDOW_DISABLED'] === 'true'));
    console.log(`Changed files: ${result.changed}\nSkipped files: ${result.skipped}`); return result.skipped === 0 ? 0 : 1;
  }
  if (command !== 'report' || directory === undefined || (flags.length !== 0 && (flags.length !== 2 || flags[0] !== '--snapshot'))) {
    console.error('Usage: agindow export-source-fix <local-request.json> --as-of <UTC>\n       agindow generate-source-patch <local-request.json> --as-of <UTC>\n       agindow bind-source <local-request.json>\n       agindow resolve-source <local-request.json>\n       agindow rotation-readiness <events.ndjson> <facts.ndjson> --as-of <UTC> [--json] [--require=verifiable]\n       agindow report <directory> [--snapshot <local-file.json>]\n       agindow codegen <local-contracts.json>\n       agindow scan <directory> --snapshot <local-contracts.json>'); return 1;
  }
  const snapshot = flags[1];
  const result = report(directory, snapshot === undefined ? {} : { snapshot });
  if (__AGINDOW_BANNER__.length > 0) console.log(__AGINDOW_BANNER__);
  console.log(result.text); return 0;
}
void main(process.argv.slice(2)).then(code => { process.exitCode = code; }).catch(() => { console.error('AGINDOW: report unavailable; no project files were changed.'); process.exitCode = 1; });
