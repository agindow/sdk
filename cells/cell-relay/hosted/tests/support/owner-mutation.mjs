import { readFile } from 'node:fs/promises';
/** Fixture-only source mutations, applied to the ephemeral bundle; the workspace stays intact.
 * @param {string|undefined} selected
 * @returns {import('esbuild').Plugin[]}
 */
export function ownerMutation(selected) {
  if (selected === undefined) return [];
  /** @type {Record<string, readonly [string, string]>} */
  const mutations = {
    fencing: ['!this.ctx.id.equals(this.env.AUTHORITY.idFromName(row.owner))', 'false'],
    uniqueness: ['SELECT id FROM assertion_fence WHERE id=? AND expires>?', 'SELECT id FROM assertion_fence WHERE id=? AND expires<?'],
    retention: ['expires + 30', 'expires'],
    quarantine: ['state.birth + 360', 'state.birth'],
  };
  const replacement = mutations[selected];
  if (replacement === undefined) throw Error('unknown owner mutation');
  return [{ name: 'owner-mutation', setup(plugin) {
    plugin.onLoad({ filter: /owner\/owner\.ts$/ }, async ({ path }) => {
      const source = await readFile(path, 'utf8');
      if (source.split(replacement[0]).length !== 2) throw Error('mutation target changed');
      return { contents: source.replace(replacement[0], replacement[1]), loader: 'ts' };
    });
  } }];
}
