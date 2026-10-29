import { sdkTabs } from './examples.js';
export const minimalConfiguration = sdkTabs(`import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe', allowedTools: ['crm.read'], buffer: 100 });
await sdk.ready;
const read = sdk.guard(() => ({ id: 'customer-123' }), {
  tool: 'crm.read', operation: 'read',
});
console.log(read()); // Local fixture; no vendor call or configured detector.
console.log(sdk.recent()); // Local metadata, not a durable audit archive.`, `from agindow import Agindow
sdk = Agindow(mode="observe", allowed_tools=("crm.read",), buffer=100)
assert sdk.ready, sdk.load_error
@sdk.guard(tool="crm.read", operation="read")
def read() -> dict[str, str]:
    return {"id": "customer-123"}
print(read())  # Local fixture; no vendor call or configured detector.
print(sdk.recent())  # Local metadata, not a durable audit archive.
sdk.close()`);

export const compactDetection = sdkTabs(`import { createAgindow } from 'agindow';
import type { StatisticalSelection } from 'agindow';
// Labelled magnitude-shift fixture; repeated values are not production calibration.
const documents = (value: number): string[] =>
  Array.from({ length: 128 }, () => JSON.stringify({ value }));
const selection: StatisticalSelection = {
  tenant: 'demo', connection: 'local', vendor: 'synthetic',
  operation: 'demo.detect', segment: 'default',
  fields: [{ side: 'request', path: '/value', enumeration: true,
    baseline: documents(1), live: documents(100) }],
};
const sdk = createAgindow({ mode: 'observe', selection });
await sdk.ready;
const input = { value: 100 };
const result = sdk.guard((body: object) => body, { tool: 'demo.detect', operation: 'read' })(input);
if (result !== input || !sdk.recent().some(event => event.type === 'drift')) {
  throw Error('Expected an unchanged result and configured drift event');
}
console.log('Configured drift observed; original value retained');`, `import json
from agindow import Agindow
# Labelled magnitude-shift fixture; repeated values are not production calibration.
def documents(value: int) -> list[str]:
    return [json.dumps({"value": value}) for _ in range(128)]
selection: dict[str, object] = {
    "tenant": "demo", "connection": "local", "vendor": "synthetic",
    "operation": "demo.detect", "segment": "default",
    "fields": [{"side": "request", "path": "/value", "enumeration": True,
                "baseline": documents(1), "live": documents(100)}],
}
sdk = Agindow(mode="observe", selection=selection)
assert sdk.ready, sdk.load_error
@sdk.guard(tool="demo.detect", operation="read")
def inspect(body: dict[str, int]) -> dict[str, int]:
    return body
value = {"value": 100}
assert inspect(value) is value
assert any(event.type == "drift" for event in sdk.recent())
sdk.close()
print("Configured drift observed; original value retained")`);

export const modeConfiguration = sdkTabs(`import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
sdk.configure({ mode: 'observe' });
sdk.configure({ mode: 'repair' });
sdk.configure({ mode: 'strict' });
// Policy changes only; no detector or repair rule is supplied here.`, `from agindow import Agindow
sdk = Agindow(mode="observe")
sdk.configure(mode="observe")
sdk.configure(mode="repair")
sdk.configure(mode="strict")
sdk.close()
# Policy changes only; no detector or repair rule is supplied here.`);
