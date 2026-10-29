import { cliPage } from './cli-guide.js';
import { nodeSdk } from './sdk-node.js';
import { pythonSdk } from './sdk-python.js';
import { nodeReference, pythonReference } from './sdk-reference.js';
import { remoteSdk } from './sdk-methods.js';
import type { DocPage, DocSection } from './model.js';
import { contract } from './model.js';
import { sdkMatrix } from './guides.js';
const reference: DocSection = { heading: 'Related SDK workflows', aliases: ['continue'], text: 'Use the pinned package for your application runtime and choose embedded or remote execution explicitly.', links: [contract('2-the-endpoint--one-route-three-modes--settled'), { label: 'Quickstart', href: '/docs/quickstart' }, { label: 'Guard HTTP reference', href: '/docs/api/guard' }] };
const page = (slug: string, title: string, sections: readonly DocSection[]): DocPage => ({ family: 'sdks', version: 'v1', slug, path: slug === 'index' ? '/docs/sdk' : `/docs/sdk/${slug}`, title, published: true, ...(slug === 'index' ? { navTitle: 'Overview' } : slug === 'node' ? { navTitle: 'Node / TypeScript' } : slug === 'cli' ? { navTitle: 'CLI and offline reports' } : {}), sections: [...sections, reference] });
export const sdks: readonly DocPage[] = [
  page('index', 'API drift documentation — SDK index', [
    { heading: 'Version-pinned packages', text: 'These pages describe the paired Node, Python and CLI versions below. Use the pinned package matching your application runtime. Examples are compiled/imported and executed against the built packages before documentation publication.', table: { columns: ['Package', 'Pinned version', 'Publication'], rows: Object.values(sdkMatrix).map(item => [item.name, item.version, item.published ? 'Published' : 'Prepared release']) } },
    { heading: 'Install', text: `Install with \`npm install agindow@${sdkMatrix.node.version}\` or \`pip install agindow==${sdkMatrix.python.version}\`. CLI ships in the Node package as \`agindow\`. The embedded SDK needs no separate AGINDOW credential; your application still supplies any vendor credentials its API client requires. Select and configure a detector after installation. Otherwise the statistical guest is default; configure windows to detect.`, links: [{ label: 'TypeScript / Node', href: '/docs/sdk/node' }, { label: 'Python', href: '/docs/sdk/python' }, { label: 'CLI / report', href: '/docs/sdk/cli' }, { label: 'Remote SDK calls', href: '/docs/sdk/remote' }] },
  ]),
  nodeSdk,
  pythonSdk,
  remoteSdk,
  cliPage,
  nodeReference,
  pythonReference,
];
