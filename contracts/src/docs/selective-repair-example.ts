import { sdkTabs } from './examples.js';
/** Runnable integration fixture. Authoritative names stand in for an off-path reviewed business snapshot. */
export const selectiveRepairExample = sdkTabs(`import { createAgindow } from 'agindow';
import type { RepairCheck } from 'agindow';
// Load authoritative application data and reviewed migration facts outside calls.
const names = new Map([['customer-123', 'Ada']]);
function verify(check: RepairCheck): boolean {
  const before = JSON.parse(check.before) as Record<string, unknown>;
  const after = JSON.parse(check.after) as Record<string, unknown>;
  return check.rule.migration === 'crm-name-v2-reviewed'
    && typeof after['id'] === 'string'
    && names.has(after['id'])
    && before['id'] === after['id']
    && before['contact_name'] === names.get(after['id'])
    && after['display_name'] === names.get(after['id']);
}
const sdk = createAgindow({ mode: 'observe', selectiveRepair: {
  rules: [{ tool: 'crm.create', side: 'request', from: 'contact_name', to: 'display_name',
    migration: 'crm-name-v2-reviewed', confidence: 1 }], verify,
} });
await sdk.ready;
// Local fixture callable; replace with your existing API client after review.
const create = sdk.guard((payload: object) => payload, { tool: 'crm.create', operation: 'write' });
const input = { id: 'customer-123', contact_name: 'Ada' };
if (create(input) !== input) throw Error('Observe changed the input');
sdk.configure({ mode: 'repair' });
const repaired = create(input) as Record<string, unknown>;
if (repaired['display_name'] !== 'Ada' || 'contact_name' in repaired) throw Error('Repair was not applied');
if (!sdk.recent().some(event => event.type === 'repair')) throw Error('Repair event missing');
console.log('Reviewed local rename verified and applied');`, `import json
from agindow import Agindow, Config, RenameRule, RepairCheck, SelectiveRepair
# Load authoritative application data and reviewed migration facts outside calls.
names = {"customer-123": "Ada"}
def verify(check: RepairCheck) -> bool:
    before = json.loads(check.before)
    after = json.loads(check.after)
    return bool(check.rule.migration == "crm-name-v2-reviewed"
                and after.get("id") in names and before.get("id") == after.get("id")
                and before.get("contact_name") == names[after["id"]]
                and after.get("display_name") == names[after["id"]])
sdk = Agindow(Config(mode="observe"), selective_repair=SelectiveRepair(
    rules=(RenameRule(tool="crm.create", side="request", source="contact_name", target="display_name",
                      migration="crm-name-v2-reviewed", confidence=1.0),), verify=verify))
assert sdk.ready, sdk.load_error
# Local fixture callable; replace with your existing API client after review.
@sdk.guard(tool="crm.create", operation="write")
def create(payload: dict[str, str]) -> dict[str, str]:
    return payload
value = {"id": "customer-123", "contact_name": "Ada"}
assert create(value) is value
sdk.configure(mode="repair")
assert create(value) == {"id": "customer-123", "display_name": "Ada"}
assert any(event.type == "repair" for event in sdk.recent())
print("Reviewed local rename verified and applied")`);
