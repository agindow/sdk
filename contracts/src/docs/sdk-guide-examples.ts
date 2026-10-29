import type { DocSnippet } from './model.js';
import { sdkMatrix } from './sdk-matrix.generated.js';
export const nodeCode = (code: string, label: string, matrix = 'node-local'): DocSnippet => ({ samples: [
  { label, code, language: 'ts', matrix: `${matrix}/${sdkMatrix.node.version}` },
] });
export const pythonCode = (code: string, label: string, matrix = 'python-local'): DocSnippet => ({ samples: [
  { label, code, language: 'python', matrix: `${matrix}/${sdkMatrix.python.version}` },
] });
export const nodeInitialize = nodeCode(`import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe' });
await sdk.ready; // Wait for the loading attempt before guarding calls.`, 'initialize.mts · TypeScript');
export const pythonInitialize = pythonCode(`from agindow import Agindow
sdk = Agindow(mode="observe")
print("Engine loaded:", sdk.ready)
if sdk.load_error is not None:
    print("Inspection unavailable; guarded calls will bypass.")
sdk.close()`, 'initialize.py · Python');

export const nodeApiCall: DocSnippet = { samples: [{
  label: 'orders.mts · TypeScript', language: 'ts', matrix: `node-quickstart/${sdkMatrix.node.version}`,
  code: `import { createAgindow } from 'agindow';

const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
sdk.on('drift', event => console.log('Drift:', event.verdict));

const getOrder = sdk.guard(async () => {
  const url = process.env.API_URL;
  if (!url) throw new Error('Set API_URL to your order JSON endpoint.');
  const response = await fetch(url, { signal: AbortSignal.timeout(10_000) });
  if (!response.ok) throw new Error(\`API returned \${response.status}\`);
  const order: unknown = await response.json();
  return order;
}, { tool: 'orders.get', operation: 'read' });

const order = await getOrder();
console.log('API result:', order);
console.log('AGINDOW events:', sdk.recent().map(event => event.type));`,
}] };
export const pythonApiCall: DocSnippet = { samples: [{
  label: 'orders.py · Python', language: 'python', matrix: `python-quickstart/${sdkMatrix.python.version}`,
  code: `import json
import os
from urllib.request import urlopen
from agindow import Agindow

sdk = Agindow(mode="observe")
sdk.on("drift", lambda event: print("Drift:", event.verdict))

@sdk.guard(tool="orders.get", operation="read")
def get_order() -> object:
    with urlopen(os.environ["API_URL"], timeout=10) as response:
        order: object = json.load(response)
        return order

try:
    print("API result:", get_order())
    print("AGINDOW events:", [event.type for event in sdk.recent()])
finally:
    sdk.close()`,
}] };
export const pythonAsyncCall: DocSnippet = { samples: [{
  label: 'orders_async.py · Python', language: 'python', matrix: `python-quickstart/${sdkMatrix.python.version}`,
  code: `import asyncio
import json
import os
from urllib.request import urlopen
from agindow import Agindow

sdk = Agindow(mode="observe")

def fetch_order() -> object:
    with urlopen(os.environ["API_URL"], timeout=10) as response:
        order: object = json.load(response)
        return order

@sdk.guard(tool="orders.get", operation="read")
async def get_order() -> object:
    return await asyncio.to_thread(fetch_order)

async def main() -> None:
    print("API result:", await get_order())
    print("AGINDOW events:", [event.type for event in sdk.recent()])

try:
    asyncio.run(main())
finally:
    sdk.close()`,
}] };
export const nodeEvents = nodeCode(`import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
const unsubscribe = sdk.on('drift', event => {
  console.log(event.tool, event.phase, event.verdict?.findings);
});
sdk.on('repair', event => console.log('Applied repair:', event.byte_identical));
// Subscribe before your guarded calls, then inspect the bounded local history.
console.log(sdk.recent());
unsubscribe();`, 'events.mts · TypeScript');
export const pythonEvents = pythonCode(`from agindow import Agindow, Event
sdk = Agindow(mode="observe")

def on_drift(event: Event) -> None:
    print(event.tool, event.phase, event.drift_class)

unsubscribe = sdk.on("drift", on_drift)
sdk.on("repair", lambda event: print("Applied repair:", event.byte_identical))
# Subscribe before guarded calls, then inspect the bounded local history.
print(sdk.recent())
unsubscribe()
sdk.close()`, 'events.py · Python');

export const nodeRepair = nodeCode(`import { createAgindow } from 'agindow';
import type { RepairCheck } from 'agindow';
// Reviewed application snapshot, loaded independently before the call.
const names = new Map([['customer-123', 'Ada']]);
function verify(check: RepairCheck): boolean {
  const before = JSON.parse(check.before) as Record<string, unknown>;
  const after = JSON.parse(check.after) as Record<string, unknown>;
  return check.rule.migration === 'crm-name-v2-reviewed'
    && typeof after['id'] === 'string' && names.has(after['id'])
    && before['id'] === after['id']
    && before['contact_name'] === names.get(after['id'])
    && after['display_name'] === names.get(after['id']);
}
const sdk = createAgindow({ mode: 'observe', selectiveRepair: {
  rules: [{ tool: 'crm.customer', side: 'response', from: 'contact_name',
    to: 'display_name', migration: 'crm-name-v2-reviewed', confidence: 1 }], verify,
} });
await sdk.ready;
// Local illustration of a reviewed migration, not a vendor request.
const customer = sdk.guard(() => ({ id: 'customer-123', contact_name: 'Ada' }),
  { tool: 'crm.customer', operation: 'read' });
sdk.configure({ mode: 'repair' }); // After evaluating the rule in observe.
console.log(customer());
console.log(sdk.recent().filter(event => event.type === 'repair'));`, 'repair.mts · TypeScript', 'node-repair');
export const pythonRepair = pythonCode(`import json
from agindow import Agindow, RenameRule, RepairCheck, SelectiveRepair
# Reviewed application snapshot, loaded independently before the call.
names = {"customer-123": "Ada"}
def verify(check: RepairCheck) -> bool:
    before = json.loads(check.before)
    after = json.loads(check.after)
    return bool(check.rule.migration == "crm-name-v2-reviewed"
                and after.get("id") in names and before.get("id") == after.get("id")
                and before.get("contact_name") == names[after["id"]]
                and after.get("display_name") == names[after["id"]])
sdk = Agindow(mode="observe", selective_repair=SelectiveRepair(
    rules=(RenameRule(tool="crm.customer", side="response", source="contact_name",
                      target="display_name", migration="crm-name-v2-reviewed", confidence=1.0),),
    verify=verify))
# Local illustration of a reviewed migration, not a vendor request.
@sdk.guard(tool="crm.customer", operation="read")
def customer() -> dict[str, str]:
    return {"id": "customer-123", "contact_name": "Ada"}
try:
    sdk.configure(mode="repair")  # After evaluating the rule in observe.
    print(customer())
    print([event for event in sdk.recent() if event.type == "repair"])
finally:
    sdk.close()`, 'repair.py · Python', 'python-repair');
