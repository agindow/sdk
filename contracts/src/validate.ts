import type { DashboardRead } from './generated/dashboard_read.js';
import type { DashboardQuery } from './generated/dashboard_query.js';
import type { DashboardError } from './generated/dashboard_error.js';
import {checkPeerObservation} from './peer.js';
import type { ShadowReview } from './generated/shadow_review.js';
import { checkShadowReview } from './shadow.js';
import type { ReviewCase } from './generated/review_case.js';
import type { ReviewResolution } from './generated/review_resolution.js';
import type { RecoveryEvent } from './generated/recovery_event.js';
import type { Notification } from './generated/notification.js';
import { copyReview, freezeReview, checkReviewCase, checkReviewResolution } from './review.js';
import { notificationText, copyNotification, checkNotification } from './notification.js';
import { freezeValue, copyBounded } from './bounds.js';
import { checkInvestigation, INVESTIGATION_BYTES } from './investigation.js';
import type { InvestigationJob } from './generated/investigation_job.js';
/**
 * Runtime validation for every cross-cell message (TI-02 AC 3).
 *
 * Producers validate what they are about to send; consumers validate what they
 * received. This module is the same code on both sides of every boundary, so
 * “what the producer allows” and “what the consumer accepts” are one
 * definition, not two that drift apart.
 */

import type { ValidateFunction } from 'ajv/dist/2020.js';
import { validatorFor } from './compile.js';
import type { AuditArtifact } from './generated/audit_artifact.js';
import type { ContractSnapshot } from './generated/contract_snapshot.js';
import type { DriftEvent } from './generated/drift_event.js';
import type { FieldProfile } from './generated/field_profile.js';
import type { ProfileSnapshot } from './generated/profile_snapshot.js';
import type { Finding } from './generated/finding.js';
import { checkFinding, checkFindingCitations } from './finding.js';
import type { ContractName } from './schemas.js';


/** A message that does not satisfy its contract. Thrown at the boundary — never swallowed into a half-valid object. */
export class ContractViolationError extends Error {
  readonly contract: ContractName;
  readonly failures: readonly string[];

  constructor(contract: ContractName, failures: readonly string[]) {
    super(`invalid ${contract}: ${failures.join('; ')}`);
    this.name = 'ContractViolationError';
    this.contract = contract;
    this.failures = failures;
  }
}

function parse<T>(contract: ContractName, value: unknown): T {
  const validate = validatorFor(contract);
  if (!validate(value)) {
    throw new ContractViolationError(contract, failures(validate));
  }
  // The one cast: ajv has proven the value at runtime; TypeScript cannot see through that.
  return value as T;
}

function failures(validate: ValidateFunction): string[] {
  return (validate.errors ?? []).map((error) => {
    const at = error.instancePath === '' ? '(root)' : error.instancePath;
    return `${at} ${error.message ?? 'is invalid'}`;
  });
}

export function parseDriftEvent(value: unknown): DriftEvent {
  return parse<DriftEvent>('drift_event', value);
}

export function parseContractSnapshot(value: unknown): ContractSnapshot {
  return parse<ContractSnapshot>('contract_snapshot', value);
}

export function parseFinding(value: unknown): Finding {
  const result = parse<Finding>('finding', copyBounded(value, { bytes: 32768, nodes: 2048, depth: 8 }));
  checkFinding(result); return freezeValue(result);
}

export function parseAuditArtifact(value: unknown): AuditArtifact {
  return parse<AuditArtifact>('audit_artifact', value);
}

export function parseFieldProfile(value: unknown): FieldProfile {
  return parse<FieldProfile>('field_profile', value);
}

export function parseProfileSnapshot(value: unknown): ProfileSnapshot {
  return parse<ProfileSnapshot>('profile_snapshot', value);
}

export function parseRecoveryEvent(value: unknown): RecoveryEvent {
  return parse<RecoveryEvent>('recovery_event', value);
}

export function parseNotification(value: unknown): Notification {
  try {
    const result = parse<Notification>('notification', copyNotification(value));
    checkNotification(result);
    return freezeValue(result);
  } catch (error) {
    if (error instanceof ContractViolationError) throw error;
    throw new ContractViolationError('notification', [error instanceof Error ? error.message : 'invalid notification']);
  }
}

export function parseReviewCase(value: unknown): ReviewCase {
  try {
    const result = parse<ReviewCase>('review_case', copyReview(value));
    checkReviewCase(result);
    return freezeReview(result);
  } catch (error) {
    if (error instanceof ContractViolationError) throw error;
    throw new ContractViolationError('review_case', [error instanceof Error ? error.message : 'invalid review']);
  }
}
export function parseReviewResolution(value: unknown): ReviewResolution {
  try {
    const result = parse<ReviewResolution>('review_resolution', copyReview(value));
    checkReviewResolution(result);
    return freezeReview(result);
  } catch (error) {
    if (error instanceof ContractViolationError) throw error;
    throw new ContractViolationError('review_resolution', [error instanceof Error ? error.message : 'invalid review']);
  }
}

export function parseShadowReview(value: unknown): ShadowReview {
  try {
    const result = parse<ShadowReview>('shadow_review', copyReview(value));
    checkShadowReview(result);
    return freezeReview(result);
  } catch (error) {
    if (error instanceof ContractViolationError) throw error;
    throw new ContractViolationError('shadow_review', [error instanceof Error ? error.message : 'invalid shadow review']);
  }
}

export function parseInvestigationJob(value: unknown): InvestigationJob {
  const job = parse<InvestigationJob>('investigation_job', copyBounded(value, { bytes: INVESTIGATION_BYTES, depth: 4, nodes: 64 }));
  checkInvestigation(job);
  return freezeValue(job);
}

export function parseInvestigationCompletion(value:unknown):import('./generated/investigation_completion.js').InvestigationCompletion {
  const result=parse<import('./generated/investigation_completion.js').InvestigationCompletion>('investigation_completion',copyBounded(value,{bytes:32768,nodes:2048,depth:8}));
  const created=Date.parse(result.ts),expires=Date.parse(result.expires_at);
  if(!result.ts.endsWith('Z')||!result.expires_at.endsWith('Z')||expires<=created||expires-created>30*86400000
    ||!/^[a-f0-9]{64}$/u.test(result.fingerprint)||new Set(result.finding_refs).size!==result.finding_refs.length) throw new Error('Invalid completion scope/lifetime');
  checkFindingCitations(result.citations);return freezeValue(result);
}

export function parsePeerObservation(value:unknown):import('./generated/peer_observation.js').PeerObservation {
 const result=parse<import('./generated/peer_observation.js').PeerObservation>('peer_observation',copyBounded(value,{bytes:8192,nodes:128,depth:4}));
 checkPeerObservation(result);return freezeValue(result);
}

export function parseCredentialEvent(value: unknown): import('./generated/credential_event.js').CredentialEvent {
  const result = parse<import('./generated/credential_event.js').CredentialEvent>('credential_event', copyBounded(value, { bytes: 8192, nodes: 64, depth: 2 }));
  for (const reference of [result.id, result.tenant, result.connection, result.operation, result.credential_version, result.prior_version, result.prior_event, result.audit_ref]) {
    if (reference !== undefined) notificationText(reference);
  }
  return freezeValue(result);
}

export function parseDashboardRead(value: unknown): DashboardRead {
 const result = parse<DashboardRead>('dashboard_read', copyBounded(value, {bytes: 1048576, nodes: 200000, depth: 16}));
 if (result.capability.status !== 'available' && (!result.capability.reason || result.data.length !== 0 || result.next_cursor !== null)) {
  throw new ContractViolationError('dashboard_read', ['Unavailable evidence requires a reason and no rows/cursor']);
 }
 if (result.capability.status === 'available' && result.source_freshness.generated_at === null) {
  throw new ContractViolationError('dashboard_read', ['Available evidence requires an owner watermark']);
 }
 for (const row of result.data) {
  if (row.drift && new Set(row.drift.findings.map(finding => finding.id)).size !== row.drift.findings.length) {
   throw new ContractViolationError('dashboard_read', ['Duplicate finding identity']);
  }
 }
 return freezeValue(result);
}

export function parseDashboardQuery(value: unknown): DashboardQuery {return parse<DashboardQuery>('dashboard_query',value);}

export function parseDashboardError(value: unknown): DashboardError {return parse<DashboardError>('dashboard_error',value);}
