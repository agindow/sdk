# Local sidecar

The sidecar exposes the runtime contract's `POST /v1/guard` over loopback HTTP
and an optional Unix HTTP socket. The default NullEngine detects nothing.
Explicit local selections use the same statistical Rust core as the SDK.
This is an explicit tool-execution endpoint. Existing applications need to
support that envelope or adapt their callables; it does not intercept TLS.

Build and run from the repository root:
```sh
cargo build --release -p agw-http-host --bin agindow-sidecar
export AGINDOW_CONNECTIONS='{"conn_example/vendor.read":{"url":"http://127.0.0.1:8787/read","method":"POST","operation":"read"}}'
AGINDOW_SOCKET=/tmp/agindow-example.sock target/release/agindow-sidecar
```
In another terminal (the example upstream must be running):
```sh
curl --unix-socket /tmp/agindow-example.sock http://localhost/healthz
curl --unix-socket /tmp/agindow-example.sock http://localhost/v1/guard \
  -H 'Content-Type: application/json' \
  --data '{"tool":"vendor.read","connection":"conn_example","operation":"read","args":{},"mode":"observe"}'
```
All configuration is environment-based: `AGINDOW_BIND` (loopback only, default
127.0.0.1:8789), `AGINDOW_SOCKET`, `AGINDOW_INSTANCE_ID`, `AGINDOW_WORK` and
`AGINDOW_CONNECTIONS`. Bindings map `connection/tool` to a fixed URL, method,
operation, optional bound headers and optional local statistical selection.
Store third-party authorization only in the binding secret; envelope fields
cannot supply headers, credentials or arbitrary URLs. Writes require an
`idempotency_key`, forwarded as the upstream `Idempotency-Key`. The binding must
actually enforce deduplication; no distributed replay store is supplied.

`/v1/registration` exposes the immutable startup identity and allowed handles.
It does not claim remote control-plane registration. Socket paths are mode 0600;
startup refuses existing paths. Remove a stale path only after verifying its
owner is no longer running. SIGINT and SIGTERM remove the owned socket. After SIGKILL,
operators must clean the stale path; clients use fallback and the same key.

Policy halts use six-field HTTP 409 feedback. A response halt retains the
completed original bytes at the `X-Agindow-Outcome` handle, and warns against
replay. Local artifacts/outcomes are FIFO limited to 128 entries/16 MiB and are
lost on restart. Streams and incomplete captures bypass unavailable inspection.
Host/Origin checks refuse foreign browser origins and DNS-rebinding hosts.
The latency header counts active host/core work and excludes upstream waits.
Socket RTT qualification is a separate benchmark; a unit test is not a timing
claim.

Build the static Linux image from the repository root:
```sh
docker build -f cells/cell-relay/sidecar/Dockerfile -t agindow-sidecar:local .
```
The scratch image includes a Rust/musl binary and embedded TLS roots. It has no
shell, curl, Node or OpenSSL runtime dependency. `compose.yaml` shares a network
namespace with the configured application image. The DaemonSet manifest uses a
node-local Unix socket and loopback health probes; provision the
`agindow-sidecar` Secret's `connections` key before applying it. Agent pods mount
`/var/run/agindow` and must use UID 65532 to access its mode-0600 socket. Review
the init container's narrowly scoped host-directory ownership change for your
cluster. A local scratch-image build/startup is tested separately; no deployment or
production guarantee follows from source manifests or a local container.
