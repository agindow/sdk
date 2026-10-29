import catalog from '../coverage/catalog.v1.json';
import { freezeValue } from './bounds.js';
export interface Capability {
 readonly id: string; readonly changeFamily: string; readonly route: string; readonly side: readonly string[];
 readonly protocol: string; readonly sdks: readonly string[]; readonly adapters: readonly string[];
 readonly sdkAdapters: readonly { sdk: string; adapters: readonly string[] }[];
 readonly evidenceSource: readonly string[]; readonly configuration: string; readonly minimumSamples: number | null;
 readonly captureBytes: number | null; readonly verifierFacts: readonly string[]; readonly supported: boolean; readonly decisions: readonly string[];
}
export const CAPABILITIES: readonly Capability[] = freezeValue(catalog.capabilities);
export interface CoverageScope {
 sdk: string; adapter: string; route: string; side: 'operation' | 'request' | 'response';
 capabilities: readonly string[]; watermark: number; expiresAt: number; observations: number;
}
/** All audiences share identical facts. Scope must come from the authorized source, never query parameters. */
export function projectCoverage(audience: 'public' | 'docs' | 'dashboard', scope: CoverageScope | null, now: number) {
 const fresh = scope !== null && Number.isSafeInteger(now) && Number.isSafeInteger(scope.watermark)
  && scope.watermark >= 0 && scope.watermark <= now && Number.isSafeInteger(scope.expiresAt) && scope.expiresAt > now
  && Number.isSafeInteger(scope.observations) && scope.observations >= 0 && scope.route.length > 0;
 return freezeValue(CAPABILITIES.map(row => {
  const configured = fresh && row.supported && scope.capabilities.includes(row.id) && row.sdks.includes(scope.sdk)
   && row.sdkAdapters.some(binding => binding.sdk === scope.sdk && binding.adapters.includes(scope.adapter)) && row.side.includes(scope.side);
  return { ...row, audience, configured, observed: configured && scope.observations > 0, qualified: false as const,
   evidenceState: row.evidenceSource.length ? 'local-evidence' as const : 'unavailable' as const,
   route: configured ? scope.route : row.route, watermark: configured ? scope.watermark : null,
   configuredSide: configured ? scope.side : null, configuredSdk: configured ? scope.sdk : null,
   configuredAdapter: configured ? scope.adapter : null,
   observations: configured ? scope.observations : null, totalTraffic: null, productionQualified: false as const };
 }));
}
