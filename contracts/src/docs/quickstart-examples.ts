import { sdkTabs } from './examples.js';
import type { DocSnippet } from './model.js';
import { sdkMatrix } from './sdk-matrix.generated.js';

const ts = `import { createAgindow } from 'agindow';

const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
sdk.on('drift', event => console.log('Drift:', event.verdict));

async function readApi(url: string): Promise<unknown> {
  const response = await fetch(url, { signal: AbortSignal.timeout(10_000) });
  if (!response.ok) throw new Error(\`API returned \${response.status}\`);
  const result: unknown = await response.json();
  return result;
}

const read = sdk.guard(readApi, { tool: 'vendor.read', operation: 'read' });
const url = process.env.API_URL;
if (!url) throw new Error('Set API_URL to your JSON GET endpoint.');
const result = await read(url);
console.log('API result:', result);
console.log('AGINDOW events:', sdk.recent().map(event => event.type));`;

const py = `import json
import os
from urllib.request import urlopen
from agindow import Agindow

sdk = Agindow(mode="observe")
sdk.on("drift", lambda event: print("Drift:", event.verdict))

def read_api(url: str) -> object:
    with urlopen(url, timeout=10) as response:
        result: object = json.load(response)
        return result

read = sdk.guard(read_api, tool="vendor.read", operation="read")
try:
    result = read(os.environ["API_URL"])
    print("API result:", result)
    print("AGINDOW events:", [event.type for event in sdk.recent()])
finally:
    sdk.close()`;

/** Public samples perform the user's read; only publication tests inject transport. */
export const quickstartExample: DocSnippet = {
  samples: sdkTabs(ts, py).samples.map(sample => ({ ...sample,
    matrix: sample.language === 'ts'
      ? `node-quickstart/${sdkMatrix.node.version}`
      : `python-quickstart/${sdkMatrix.python.version}`,
  })),
};
