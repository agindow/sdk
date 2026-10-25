/** Reviewed commercial recommendations, not approved offers or a wire schema.
 * Shared by public copy and local launch entitlements; billing still requires
 * a separately approved immutable host catalog. No detector consumes this data.
 */
export const LAUNCH_PACKAGING = {
 version: 'launch-recommendation-v2-2026-10-21',
 status: 'provisional',
 reviewedOn: '2026-10-21',
 source: 'tasks/pricing-research.md',
 research: 'tasks/pricing-research-appendix.md',
 currency: 'USD',
 billingPeriod: 'month',
 overageCents: 0,
 monthlyCap: 'base-only',
 plans: {
  Free: { baseCents: 0, allowance: 1, connections: 1, retentionDays: 7, period: 'month' },
  Pro: { baseCents: 4900, allowance: 2, connections: 5, retentionDays: 30, period: 'month' },
  Team: { baseCents: 19900, allowance: 15, connections: 25, retentionDays: 90, period: 'quarter' },
 },
 enterprise: { name: 'Enterprise', price: 'Custom', scope: 'Agreed scope', allowance: 'Agreed allowance', retention: 'Agreed retention' },
} as const;
export type LaunchPlan = keyof typeof LAUNCH_PACKAGING.plans;
