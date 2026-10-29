import { productHeadline, productSummary, productMaturity } from './positioning.js';
import type { DocPage } from './model.js';
import { surfaceExample } from './examples.js';
export const preview: DocPage = {
  family: 'api-reference',
  version: 'v1',
  slug: 'agent-surface',
  title: 'AGINDOW agent surface',
  navTitle: 'MCP quickstart',
  published: true,
  sections: [
    {
      heading: 'Local interface preview',
      text: `${productHeadline} ${productSummary} ${productMaturity} The statistical guest is default; configure windows to detect. Without a selection it emits no findings. Coverage describes observed evidence and cannot prove every API change is safe. The local MCP interface exposes lookup_contract, query_drift, fetch_audit, simulate_repair and coverage_report. Requests require authorization; simulations never execute writes. A hosted endpoint is not available yet.`,
    },
    {
      heading: 'Runnable example', snippet: 0,
      text: 'For a configured local endpoint, set AGINDOW_SURFACE to its origin and AGINDOW_KEY to your authorized credential, then run the example below. No hosted endpoint or API key issuance is available yet.',
    },
    {
      heading: 'Read the result',
      text: 'The coverage_report tool returns evidence and explicit unavailable or unvalidated states, not a vendor response or permission to repair. Check transport/JSON-RPC errors and the tool isError flag before interpreting structuredContent. This read-only command cannot execute a vendor write.',
      links: [{ label: 'Tool results and failure layers', href: '/docs/api/tools' }],
    },
    {
      heading: 'Verify the API description',
      text: 'Get /openapi.json and verify its x-agindow-signature with an independently trusted Ed25519 public key and key_id. Remove the signature field, encode using agindow.canonical-json/1 (recursively sorted keys, JSON primitives), then verify the exact UTF-8 bytes. The signed tools contain the input and output schemas served by MCP. Never trust a public key merely because the response includes it.',
    },
  ],
  snippets: [surfaceExample],
};
