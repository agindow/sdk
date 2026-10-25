import { readFileSync, existsSync } from 'node:fs';
import { dirname, resolve, join } from 'node:path';
/** Collect licenses only for dependencies actually bundled into the CLI.
 * @param {readonly string[]} inputs @param {string} root */
export function notices(inputs, root) {
  const packages = new Map();
  for (const input of inputs) {
    if (!input.includes('node_modules/')) continue;
    let directory = dirname(resolve(root, input));
    for (;;) {
      const manifest = join(directory, 'package.json');
      if (existsSync(manifest)) {
        const metadata = /** @type {{name?: string; version?: string}} */ (JSON.parse(readFileSync(manifest, 'utf8')));
        if (typeof metadata.name === 'string' && typeof metadata.version === 'string') {
          const license = ['LICENSE', 'LICENSE.md', 'LICENSE.txt', 'license', 'license.md'].map(name => join(directory, name)).find(path => existsSync(path));
          if (license === undefined) throw Error(`missing bundled dependency license: ${metadata.name}`);
          packages.set(`${metadata.name}@${metadata.version}`, readFileSync(license, 'utf8')); break;
        }
      }
      const parent = dirname(directory); if (parent === directory) throw Error('bundled package manifest missing'); directory = parent;
    }
  }
  return [...packages.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([name, license]) => `${name}\n${'='.repeat(name.length)}\n${license}`).join('\n\n');
}
