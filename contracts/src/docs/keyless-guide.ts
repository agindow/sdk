import type { DocPage } from './model.js';
import { keylessNodeSetup } from './keyless-example.js';
import { keylessPythonSetup, keylessGenerate } from './keyless-python-example.js';
export const keylessGuide: DocPage = {
  family: 'guides',
  version: 'v1',
  slug: 'keyless-auth',
  path: '/docs/guides/keyless-auth',
  title: 'Enable keyless remote access',
  navTitle: 'Keyless remote access',
  published: true,
  sections: [
    {
      heading: 'Choose the authentication boundary',
      text: 'Embedded Node/Python execution needs no AGINDOW credential. Your existing client still supplies vendor credentials. Keyless remote access is for an explicit HTTP execution host; a dashboard session is only for interactive account administration. Fallback API keys are an explicit compatibility or break-glass option and are never required by the embedded SDK.',
      table: {
        columns: ['Path', 'Authentication', 'Setup'],
        rows: [
          [
            'Embedded SDK',
            'No separate AGINDOW credential',
            'Configure the detector in your application.',
          ],
          [
            'Self-hosted remote execution',
            'Short-lived, sender-bound token',
            'Use an enabled deployment and register a workload as described below.',
          ],
          [
            'Fallback API keys',
            'Explicit bearer credential',
            'Only for a boundary that explicitly supports the granted scopes.',
          ],
        ],
      },
      links: [
        { label: 'Embedded quickstart', href: '/docs/quickstart' },
        { label: 'Authentication overview', href: '/docs/authentication' },
      ],
    },
    {
      heading: '1. Check account and execution-host availability',
      text: 'Sign into the dashboard and choose your organization and Test environment. You need an active owner membership, recent authentication and a host-enabled self-service policy for an existing read connection. Registration is scoped to your verified membership; never paste a different tenant ID to select an organization. If the deployment is unconfigured or has no eligible read connection, registration is unavailable. This flow has been tested locally, not with a live deployment. Local implementations do not establish a managed public execution service. Embedded SDK use remains available without a dashboard session.',
    },
    {
      heading: '2. Generate the key locally',
      text: 'Run this Node script locally (for example node generate-key.mjs). It creates an Ed25519 private PEM with mode 0600 and prints only public kid/x. Never upload the private PEM, a private JWK d field or a token. For an existing key, export its public JWK locally and choose a kid. Alternatively generate with openssl genpkey -algorithm Ed25519 -out workload-private.pem under umask 077, then use Node createPrivateKey/createPublicKey to export the public JWK; never paste the PEM into the dashboard.',
      example: { language: 'ts', code: keylessGenerate },
    },
    {
      heading: '3. Dashboard: Workloads → Register workload',
      text: 'Open Workloads, separately from Fallback API keys. Under Register workload paste the public key ID (kid) and public Ed25519 coordinate (x). Review the server-selected Test connection and read tools, then choose Review registration and Confirm create. Your session authorizes registration, not execution. It cannot authenticate /v1/token or /v1/guard. The server binds the public key to your selected organization after checking membership. Other organizations cannot read or change its workloads. Do not use the public form for live or write scopes.',
    },
    {
      heading: '4. Wait for delivery and exchange readiness',
      text: 'Submitted means registration is pending. Admitted means it was approved but may not have reached the execution host. Delivered means the host acknowledged that version. Exchange ready additionally checks that the current registration and token issuer can be used. A new empty assertion ledger waits six minutes. Refresh when readiness expires; pending delivery is never ready. Readiness does not prove possession of your private key or successful upstream execution.',
    },
    {
      heading: '5. Public handoff fields',
      text: 'Once Exchange ready is current, copy the Node handoff or use the Python helper below. Public handoff fields can be viewed again: exchange URL, guard URL, registration/client ID, kid, assertion audience, connection and tools. These are not secrets. The assertion iss/sub equal the client ID; its audience is the exact exchange URL, distinct from access-token audience agindow:guard. Keep the private key file on your workload; it is never available for retrieval in AGINDOW. A lost private key requires a replacement public key.',
    },
    {
      heading: '6. Configure the Node client',
      text: 'Install the published version from the pinned Node guide. Save this helper as keyless-auth.ts and use connectSelfHosted with the ready handoff values and local privateKeyFile. connectSelfHosted is this application helper, not a package export. It uses private_key_jwt with fresh two-minute assertions with fresh jti and creates a separate ephemeral DPoP signer. It does not upload the private key. No agw_* key, dashboard cookie or implicit fallback belongs in this path.',
      example: { language: 'ts', code: keylessNodeSetup },
      links: [{ label: 'Pinned Node installation', href: '/docs/sdk/node' }],
    },
    {
      heading: '7. Configure Python RemoteAuth',
      text: 'Install the pinned Python SDK and a host Ed25519 signing library. This optional example uses cryptography (install it in your workload environment); it is not an AGINDOW dependency. Save the helper as keyless_auth.py. Call connect_self_hosted with the same public handoff values and private_key_file. It prepares RemoteAuth with workload_identity and a separate ProofSigner, acquiring and refreshing outside guarded calls. The private key file remains customer-side. Close auth when finished; do not store a permanent assertion JWT.',
      example: { language: 'python', code: keylessPythonSetup },
      links: [{ label: 'Pinned Python installation', href: '/docs/sdk/python' }],
    },
    {
      heading: '8. Run a first read in observe mode',
      text: 'Node: pass auth to guardRemote(original, {tool: admittedTool, connection: admittedConnection, operation: "read", args: {}, mode: "observe"}, {endpoint: guardUrl, auth}) and close auth in finally. Python: pass RemoteEnvelope(tool=admitted_tool, connection=admitted_connection, operation="read", args={}, mode="observe") and RemoteOptions(endpoint=guard_url, auth=auth) to guard_remote(original, envelope, options), then read/close the returned response and close auth. Use a safe configured upstream read. A successful exchange alone is not a protected read; distinguish SDK fallback events from actual host dispatch. A valid auth path does not enable a detector: the statistical guest is default; configure windows to detect.',
      links: [{ label: 'Remote SDK signatures and failures', href: '/docs/sdk/remote' }],
    },
    {
      heading: '9. Rotate public key or revoke',
      text: 'Generate the replacement Ed25519 key locally. In Workloads select Rotate public key, paste the new public kid/x, review and confirm. Rotation is revision-bound and has no automatic key overlap; wait again for delivery/readiness before switching the workload file. Revoke workload requires the current revision and confirmation. Neither retrying an old form nor the renewal job can resurrect a revoked grant. Already-issued tokens and stale leases retain bounded residual validity: conservatively up to 66 minutes under the documented host clock assumptions, not instant invalidation. Revoking the final grant stops the profile; ask the team running your deployment to review recovery before registering again.',
    },
    {
      heading: '10. Availability and recovery',
      text: 'This optional flow requires a self-hosted deployment with workload registration enabled and an approved Test/read connection. If Workloads is unavailable, use the embedded SDK or request access from the team running your deployment. New scopes and stopped or expired registrations require that team’s review. Renewals preserve existing scopes and cannot extend an expired authorization. Do not attempt to restore access by replaying a registration or bypassing readiness checks.',
    },
    {
      heading: 'Replay and deployment limits',
      text: 'Local tests exercise token exchange, proof of possession, refusal and refresh using synthetic reads. They do not establish reliability for a public deployment or an external identity provider. Restart or lost authentication state can temporarily refuse exchange and execution; refresh outside the guarded call and wait for current readiness. The standalone host keeps replay and rate-limit state process-local. A successful exchange does not configure detection, verify a write or guarantee durable outcome retrieval. Reconcile uncertain writes with the vendor before any retry.',
    },
  ],
};
