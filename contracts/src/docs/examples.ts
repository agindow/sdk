/** Exact published code is compiled and executed by the build gate. */
import type { DocCodeSample, DocSnippet } from './model.js';

const node = (code: string): DocCodeSample => ({
  label: 'ts', language: 'ts', matrix: 'node-local/0.1.0-alpha.1', code,
});
const python = (code: string): DocCodeSample => ({
  label: 'py', language: 'python', matrix: 'python-local/0.1.0a0', code,
});
/** Pair TypeScript and Python samples in one Better Auth–style tabbed block. */
export const sdkTabs = (ts: string, py: string): DocSnippet => ({ samples: [node(ts), python(py)] });
export const shellTabs = (code: string, matrix = 'local-build/1'): DocSnippet => ({
  samples: [{ label: 'sh', language: 'bash', matrix, code }],
});

const nodeFixture = `import { createAgindow } from 'agindow';
const sdk = createAgindow({ mode: 'observe' });
await sdk.ready;
const payload = { id: 'synthetic-customer' };
let calls = 0;
const read = sdk.guard(() => { calls++; return payload; }, { tool: 'vendor.read', operation: 'read' });
const result = read();
if (result !== payload || calls !== 1) throw Error('Fixture identity changed');
// A labelled unavailable-engine fixture produces one bypass without a hosted key.
const unavailable = createAgindow({ mode: 'observe' }, async () => { throw Error('Synthetic loading fault'); });
await unavailable.ready;
const fallback = unavailable.wrap(() => payload);
if (fallback() !== payload || unavailable.recent().filter(e => e.type === 'bypass').length !== 1) throw Error('Missing bypass');
sdk.configure({ mode: 'observe' });
console.log('Synthetic fixture: original result retained; unavailable engine bypassed once.');`;

const pythonFixture = `import asyncio
from agindow import Agindow
sdk = Agindow(mode="observe")
payload = {"id": "synthetic-customer"}
@sdk.guard(tool="vendor.read", operation="read")
def read() -> dict[str, str]:
    return payload
@sdk.guard(tool="vendor.read", operation="read")
async def read_async() -> dict[str, str]:
    return payload
with sdk.context(mode="observe"):
    assert read() is payload
async def main() -> None:
    async with sdk.context(mode="observe"):
        assert await read_async() is payload
asyncio.run(main())
# A closed engine is a labelled availability fixture, not vendor drift.
sdk.close()
assert read() is payload
assert len([event for event in sdk.recent() if event.type == "bypass"]) == 1
print("Synthetic fixture: sync/async identity retained; closed engine bypassed once.")`;

const observeTs = `import { createAgindow } from 'agindow';
import type { Verdict } from 'agindow';
const pass: Verdict = { halt: false, bypassed: false, class: null, findings: 0, detectors: 0, malformed: false };
const payload = { old: 'synthetic-value' };
// Synthetic candidate engine: only a fixture, not the default WASM engine.
const sdk = createAgindow({ mode: 'observe' }, async () => ({
    begin: () => ({ request: pass, close() {}, finish: () => ({ ...pass,
      class: 'shape', findings: 1, confidence: 0.99,
      repair: { verified: true, value: { current: 'synthetic-value' }, changes: [{ path: 'old', kind: 'field_renamed', to: 'current' }] },
    }) }),
}));
await sdk.ready;
const result = sdk.guard(() => payload, { tool: 'vendor.read' })();
if (result !== payload || !('old' in result)) throw Error('Observe applied a candidate');
console.log('Synthetic fixture: offered patch remained unapplied in observe.');`;

const observePy = `from agindow import Agindow
from agindow.types import Verdict, VerifiedRepair

class _Call:
    @property
    def request(self) -> Verdict:
        return Verdict()
    def finish(self, value: object) -> Verdict:
        return Verdict(
            drift_class="shape",
            findings=1,
            confidence=0.99,
            repair=VerifiedRepair(value={"current": "synthetic-value"}, changes=(("old", "field_renamed"),)),
        )
    def close(self) -> None:
        return None

class _Candidate:
    def begin(self, value: object, mode: str, operation: str, fail_closed: bool, tool: str) -> _Call:
        return _Call()

payload = {"old": "synthetic-value"}
# Synthetic candidate engine: only a fixture, not the default WASM engine.
sdk = Agindow(mode="observe", engine=_Candidate())
@sdk.guard(tool="vendor.read")
def read() -> dict[str, str]:
    return payload
result = read()
if result is not payload or "old" not in result:
    raise AssertionError("Observe applied a candidate")
print("Synthetic fixture: offered patch remained unapplied in observe.")`;

const auditTs = `import { createAgindow } from 'agindow';
import type { AuditRecord } from 'agindow';
const sdk = createAgindow({ mode: 'observe', audit: { tenant: 'synthetic', capacity: 4 } });
await sdk.ready;
const payload = { id: 'synthetic-customer' };
const result = sdk.guard(() => payload, { tool: 'vendor.read', operation: 'read' })();
if (result !== payload) throw Error('Observe changed the payload');
// Synthetic, in-memory sink only. A real host must persist before acknowledging.
const artifacts: AuditRecord[] = [];
const receipt = await sdk.drainAudit(artifact => { artifacts.push(artifact); });
if (!receipt.available || receipt.delivered !== 1 || artifacts[0]?.['decision'] !== 'ok') throw Error('Missing audit evidence');
console.log('Synthetic audit receipt', receipt, artifacts[0]?.id);`;

const auditPy = `# Guest audit FIFO drain (Node sdk.drainAudit) is not published on Python.
# This fixture still proves observe identity and retained local events.
from agindow import Agindow
sdk = Agindow(mode="observe")
payload = {"id": "synthetic-customer"}
@sdk.guard(tool="vendor.read", operation="read")
def read() -> dict[str, str]:
    return payload
result = read()
if result is not payload:
    raise AssertionError("Observe changed the payload")
print("Synthetic fixture: Python retains identity; guest audit drain remains Node-only.")`;

export { detectionExample, detectionTs } from './detection.js';
export const nodeExample = sdkTabs(nodeFixture, pythonFixture);
export const pythonExample = sdkTabs(nodeFixture, pythonFixture);
export const observeExample = sdkTabs(observeTs, observePy);
export const auditExample = sdkTabs(auditTs, auditPy);

export const cliExample = shellTabs('node cells/cell-relay/node/dist/cli.cjs report ./contracts', 'cli-local/0.1.0-alpha.1');
export const buildExample = shellTabs(`pnpm install --frozen-lockfile
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
node cells/cell-relay/node/scripts/build.mjs
python3 cells/cell-relay/python/scripts/build.py --stage-only
uv sync --directory cells/cell-relay/python --frozen`);
export const surfaceExample: DocSnippet = {
  samples: [{
    label: 'sh', language: 'bash', matrix: 'surface-mcp/0.1.0',
    code: "curl -sS \"$AGINDOW_SURFACE/mcp\" \\\n  -H \"Authorization: Bearer $AGINDOW_KEY\" \\\n  -H 'Content-Type: application/json' \\\n  -H 'Accept: application/json, text/event-stream' \\\n  -H 'MCP-Protocol-Version: 2025-03-26' \\\n  --data '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"coverage_report\",\"arguments\":{\"tools\":[\"vendor.read\"]}}}'",
  }],
};
