import { sdkReleasePrepared } from './release-status.js';
import type { DocPage } from './model.js';
import { sdkMatrix } from './guides.js';

/** Narrow assertions over product copy; negations do not become release claims. */
export function validateEditorial(page: DocPage): void {
  const text = JSON.stringify(page);
  if (/operator-only|KEYLESS_REGISTRATIONS|tenant-free host-wide policy|Publication:|contracts\/src\/docs|refreshPublicationDigest|publication\.v1\.json|\.opencode\/|\bTII-\d+/iu.test(text)) throw Error('Deployment details in developer docs');
  if (/default (?:engine|guest)(?: is| remains| stays)?[^.!?]{0,12}NullEngine|default engine detects nothing/iu.test(text)) {
    throw Error('Stale default engine claim');
  }
  if (/Production traffic is running in this playground|Fixtures are uploaded to AGINDOW servers|OpenAPI validation fails on the Unit body|sdk\.simulate proves real selective repair|Schema validation alone establishes rename intent/iu.test(text)) {
    throw Error('Misleading playground evidence');
  }
  if (/\bchallenge\b|\bdeny\b|mode[:= ]+["']?shadow\b/iu.test(text)) throw Error('Invalid product vocabulary');
  for (const value of copyStrings(page)) assertRegistryInstallClaims(value);
  if (/(?:Automatically|Blindly) retry an? (?:uncertain|completed).*write/iu.test(text)
    || /Default-guest selective repair is available|Durable replay is guaranteed/iu.test(text)) throw Error('Unsafe product claim');
  for (const section of page.sections) {
    if (section.table?.vocabulary) {
      const expected = section.table.vocabulary === 'modes' ? ['observe', 'repair', 'strict'] : ['ok', 'repaired', 'degraded', 'bypassed', 'blocked'];
      if (JSON.stringify(section.table.rows.map(row => row[0])) !== JSON.stringify(expected)) throw Error('Invalid vocabulary table');
    }
    for (const link of section.links ?? []) {
      if (link.href === '/SKILL.md') continue;
      if (!/^\/docs(?:\/|$)|^\/integrations\/[a-z-]+$|^\/try$|^\/#(?:maturity|detect)$|^https:\/\/github\.com\/agindow\/sdk(?:\/|$)/u.test(link.href)) {
        throw Error('Unreviewed docs link');
      }
    }
  }
}

/** Inspect source strings so JSON-escaped newlines never become part of a version. */
function copyStrings(value: unknown): readonly string[] {
  if (typeof value === 'string') return [value];
  return value !== null && typeof value === 'object' ? Object.values(value).flatMap(copyStrings) : [];
}

/** Registry commands must match the reviewed pair; prepared builds carry a global availability notice. */
function assertRegistryInstallClaims(text: string): void {
  const commands = /\b(npm\s+(?:install|add)|pip3?\s+install)\s+(?:--?[\w-]+\s+)*["'`]?agindow(?=[@=<>!~^\s"'`;,.\])]|$)([^\s"'`;,\])]*)/gu;
  for (const match of text.matchAll(commands)) {
    const npm = match[1]?.startsWith('npm') === true;
    const sdk = npm ? sdkMatrix.node : sdkMatrix.python;
    const specifier = match[2]?.replace(/\.+$/u, '') ?? '';
    if ((!sdk.published && !sdkReleasePrepared) || specifier !== `${npm ? '@' : '=='}${sdk.version}`) {
      throw Error('Unpinned, mismatched or unpublished registry install claim');
    }
  }
}
