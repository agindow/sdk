import { docsChannels, sdkMatrix } from './sdk-matrix.generated.js';
const isPreparedChannel = (label: string | undefined): boolean => label === 'Prepared release';
export const sdkReleasePrepared = isPreparedChannel(docsChannels[0]?.label) && (!sdkMatrix.node.published || !sdkMatrix.python.published);
export const sdkReleaseNotice = sdkReleasePrepared
  ? `Prepared release: npm ${sdkMatrix.node.version} and PyPI ${sdkMatrix.python.version} are prepared for maintainer publication, but are not yet available in the registries. Registry install commands below apply after publication. See the SDK changelog for changes.`
  : '';
export const sdkDistributionStatus = sdkReleasePrepared ? 'prepared for publication' : 'published';
/** Last registry-confirmed pair while the next pair is prepared locally. */
export const publishedSdkMatrix = {
  node: sdkMatrix.node.published ? sdkMatrix.node : { version: '0.1.0-alpha.2', published: true, name: 'agindow' },
  python: sdkMatrix.python.published ? sdkMatrix.python : { version: '0.1.0a1', published: true, name: 'agindow' },
  cli: sdkMatrix.cli.published ? sdkMatrix.cli : { version: '0.1.0-alpha.2', published: true, name: 'agindow report' },
} as const;
