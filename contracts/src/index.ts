export { CONTRACTS, SCHEMA_FILES, type ContractName } from './schemas.js';
export {
  ContractViolationError,
  parseAuditArtifact,
  parseContractSnapshot,
  parseDriftEvent,
  parseFinding,
  parseFieldProfile,
  parseNotification,
  parseProfileSnapshot,
  parseRecoveryEvent,
} from './validate.js';
export type { AuditArtifact } from './generated/audit_artifact.js';
export type { ContractSnapshot } from './generated/contract_snapshot.js';
export type { DriftEvent } from './generated/drift_event.js';
export type { Finding } from './generated/finding.js';
export type { FieldProfile } from './generated/field_profile.js';
export type { ProfileSnapshot } from './generated/profile_snapshot.js';
export type { Notification } from './generated/notification.js';
export type { RecoveryEvent } from './generated/recovery_event.js';

export { parseReviewCase, parseReviewResolution } from './validate.js';
export type { ReviewCase, ReviewPhase, ReviewScope } from './generated/review_case.js';
export type { ReviewResolution } from './generated/review_resolution.js';
export { REVIEW_BYTES, reviewText, freezeReview, canonicalReview } from './review.js';
export {
  NOTIFICATION_BYTES,
  NOTIFICATION_IDENTIFIER_BYTES,
  NOTIFICATION_REFS,
  checkNotification,
  notificationText,
  suspiciousContent,
} from './notification.js';
export { copyBounded, freezeValue, canonicalJson, type JsonBounds } from './bounds.js';

export { parseShadowReview } from './validate.js';
export type { ShadowReview, ShadowCase, ShadowResolution } from './generated/shadow_review.js';
export { reviewPlanText } from './shadow.js';
export { parseInvestigationJob } from './validate.js';
export type { InvestigationJob } from './generated/investigation_job.js';
export { INVESTIGATION_BYTES, INVESTIGATION_TTL_MS, microUnits } from './investigation.js';

export { parseInvestigationCompletion } from './validate.js';
export type { InvestigationCompletion } from './generated/investigation_completion.js';

export {parsePeerObservation} from './validate.js';
export type {PeerObservation} from './generated/peer_observation.js';
export {exactI64} from './peer.js';

export { parseCredentialEvent } from './validate.js';
export type { CredentialEvent } from './generated/credential_event.js';

export { parseDashboardRead } from './validate.js';
export type { DashboardRead } from './generated/dashboard_read.js';

export { parseDashboardQuery } from './validate.js';
export type { DashboardQuery } from './generated/dashboard_query.js';

export { parseDashboardError } from './validate.js';
export type { DashboardError } from './generated/dashboard_error.js';
export { inspectAuditText, patchSides, AUDIT_INSPECTION_BYTES, type AuditInspection } from './audit-inspection.js';

export { jsonText, type JsonText } from './json-text.js';

export type { SourceRepairEvidence } from './generated/source_repair_evidence.js';
export {
  SOURCE_REPAIR_BYTES, SOURCE_REPAIR_TTL_MS, parseSourceRepairEvidence, validateSourceRepairEvidence,
  checkSourceRepairEvidence, sourceRepairDigest, sameSourceRepairScope, boundedSourceRepair,
  sourceRepairFailure, refuse, type SourceRepairAuthority, type SourceRepairRefusal, type SourceRepairResult,
} from './source-repair.js';
export { readPlaygroundReview, readPlaygroundCatalog, playgroundId, playgroundOutcome,
  type PlaygroundReview, type PlaygroundScope } from './playground.js';
