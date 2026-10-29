import { sdkTabs } from './examples.js';
/** Local protocol fixture: a real HTTP exchange, without claiming engine inspection. */
export const remoteExample = sdkTabs(`import { createServer } from 'node:http';
import { guardRemote } from 'agindow';
import type { RemoteEnvelope } from 'agindow';
let originalCalls = 0;
// Original application read. Replace with your existing client in an integration.
const readCustomer = (input: RemoteEnvelope): Response => {
  originalCalls++;
  return Response.json({ id: (input.args as { id: string }).id, name: 'Ada' });
};
// Protocol fixture only: fixed read binding, no AGINDOW engine or vendor call.
const host = createServer(async (request, response) => {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(Buffer.from(chunk));
  const input = JSON.parse(Buffer.concat(chunks).toString()) as RemoteEnvelope;
  if (request.url !== '/v1/guard' || input.tool !== 'crm.read'
    || input.connection !== 'local' || input.operation !== 'read') {
    response.writeHead(400).end(); return;
  }
  response.writeHead(200, { 'content-type': 'application/json' });
  response.end(JSON.stringify({ id: (input.args as { id: string }).id, name: 'Ada' }));
});
await new Promise<void>(resolve => host.listen(0, '127.0.0.1', resolve));
const address = host.address();
if (!address || typeof address === 'string') throw Error('Missing local address');
const options = { endpoint: \`http://127.0.0.1:\${address.port}/v1/guard\`, timeoutMs: 1000 };
const envelope: RemoteEnvelope = {
  tool: 'crm.read', connection: 'local', operation: 'read',
  args: { id: 'customer-123' }, mode: 'observe',
};
try {
  const result = await guardRemote(readCustomer, envelope, options);
  const customer = await result.json() as { id: string };
  if (result.status !== 200 || customer.id !== 'customer-123' || originalCalls !== 0) throw Error('Remote read failed');
  console.log(result.status, customer); // 200, customer data
} finally {
  await new Promise<void>((resolve, reject) => host.close(error => error ? reject(error) : resolve()));
}
// The unavailable endpoint now invokes the original read, once for this call.
const fallback = await guardRemote(readCustomer, envelope, options);
if (fallback.status !== 200 || Number(originalCalls) !== 1) throw Error('Fallback failed');
console.log('Original read returned after transport failure');`, `import asyncio
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread
from agindow import (RemoteEnvelope, RemoteOptions, RemoteResponse,
                     guard_remote, guard_remote_async)

original_calls = 0
# Original application read; replace with your existing client in an integration.
def read_customer(input: RemoteEnvelope) -> RemoteResponse:
    global original_calls
    original_calls += 1
    body = json.dumps({"id": "customer-123", "name": "Ada"}).encode()
    return RemoteResponse(200, {"content-type": "application/json"}, body)

# Protocol fixture only: fixed read binding, no engine or vendor call.
class Host(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        value = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if (self.path != "/v1/guard" or value["tool"] != "crm.read"
                or value["connection"] != "local" or value["operation"] != "read"):
            self.send_error(400)
            return
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps({"id": value["args"]["id"], "name": "Ada"}).encode())
    def log_message(self, format: str, *args: object) -> None:
        pass

host = ThreadingHTTPServer(("127.0.0.1", 0), Host)
thread = Thread(target=host.serve_forever, daemon=True)
thread.start()
envelope = RemoteEnvelope(tool="crm.read", connection="local", operation="read",
                          args={"id": "customer-123"}, mode="observe")
options = RemoteOptions(endpoint=f"http://127.0.0.1:{host.server_port}/v1/guard",
                        timeout_seconds=1)
def show(result: RemoteResponse) -> None:
    try:
        customer = json.loads(result.read())
        assert result.status == 200 and customer["id"] == "customer-123"
        print(result.status, customer)
    finally:
        result.close()
async def main() -> None:
    # Async helper takes the same synchronous fallback, running in a worker thread.
    show(await guard_remote_async(read_customer, envelope, options))
try:
    show(guard_remote(read_customer, envelope, options))
    asyncio.run(main())
    assert original_calls == 0
finally:
    host.shutdown()
    host.server_close()
    thread.join()
# Closed endpoint: original read supplies this call's value.
show(guard_remote(read_customer, envelope, options))
assert original_calls == 1`);

// Configuration recipes: injected host credentials, no network or secret in the sample.
export const remoteCredentials = sdkTabs(`import type { RemoteOptions } from 'agindow';
// Pass a host-issued key from secret storage, never a vendor API key.
export function authenticatedOptions(endpoint: string, apiKey: string): RemoteOptions {
  return { endpoint, apiKey, timeoutMs: 5000,
    onAuthDiagnostic: reason => console.log(reason),
  };
}`, `from agindow import RemoteOptions
# Pass a host-issued key from secret storage, never a vendor API key.
def authenticated_options(endpoint: str, api_key: str) -> RemoteOptions:
    return RemoteOptions(endpoint=endpoint, api_key=api_key, timeout_seconds=5,
                         on_auth_diagnostic=print)`);
