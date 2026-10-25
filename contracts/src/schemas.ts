/**
 * The schemas themselves — the source of truth every cell validates against.
 *
 * These are the same files `cell-relay`'s build script turns into Rust types,
 * so the TS and Rust views cannot drift from each other (TI-02).
 */

import DashboardErrorSchema from '../dashboard_error.v1.json';
import DashboardQuerySchema from '../dashboard_query.v2.json';
import dashboardRead from '../dashboard_read.v2.json';
import credentialEvent from '../credential_event.v1.json';
import type { Schema } from 'ajv';
import peerObservation from '../peer_observation.v1.json';
import investigationCompletion from '../investigation_completion.v1.json';
import investigationJob from '../investigation_job.v1.json';
import shadowReview from '../shadow_review.v1.json';
import notification from '../notification.v1.json';
import reviewCase from '../review_case.v1.json';
import reviewResolution from '../review_resolution.v1.json';
import recoveryEvent from '../recovery_event.v1.json';
import auditArtifact from '../audit_artifact.v1.json';
import contractSnapshot from '../contract_snapshot.v2.json';
import driftEvent from '../drift_event.v1.json';
import fieldProfile from '../field_profile.v1.json';
import profileSnapshot from '../profile_snapshot.v1.json';
import finding from '../finding.v2.json';

export type ContractName = 'dashboard_error' | 'dashboard_query' | 'dashboard_read' | 'credential_event' | 'peer_observation' | 'investigation_completion' | 'investigation_job' | 'shadow_review' | 'notification' | 'review_case' | 'review_resolution' | 'recovery_event' | 'drift_event' | 'contract_snapshot' | 'finding' | 'audit_artifact' | 'field_profile' | 'profile_snapshot';

/**
 * Which version file each schema above was imported from.
 *
 * The generators read `contracts/manifest.json`; this module is hand-written,
 * so without a machine-checkable link a release that bumps the manifest would
 * leave the validators compiling the *previous* schema — v2 types checked
 * against v1 rules, with every suite green. `tests/unit/contracts.test.ts`
 * asserts this record matches the manifest.
 */
export const SCHEMA_FILES: Record<ContractName, string> = {
  dashboard_error: 'dashboard_error.v1.json',
  dashboard_query: 'dashboard_query.v2.json',
  dashboard_read: 'dashboard_read.v2.json',
  credential_event: 'credential_event.v1.json',
  peer_observation: 'peer_observation.v1.json',
  investigation_completion: 'investigation_completion.v1.json',
  investigation_job: 'investigation_job.v1.json',
  shadow_review: 'shadow_review.v1.json',
  notification: 'notification.v1.json',
  review_case: 'review_case.v1.json',
  review_resolution: 'review_resolution.v1.json',
  recovery_event: 'recovery_event.v1.json',
  profile_snapshot: 'profile_snapshot.v1.json',
  field_profile: 'field_profile.v1.json',
  drift_event: 'drift_event.v1.json',
  contract_snapshot: 'contract_snapshot.v2.json',
  finding: 'finding.v2.json',
  audit_artifact: 'audit_artifact.v1.json',
};

/** Keyed by contract name; `contracts/manifest.json` is the version registry the code generators read. */
export const CONTRACTS: Record<ContractName, Schema> = {
  dashboard_error: DashboardErrorSchema,
  dashboard_query: DashboardQuerySchema,
  dashboard_read: dashboardRead,
  credential_event: credentialEvent,
  peer_observation: peerObservation,
  investigation_completion: investigationCompletion,
  investigation_job: investigationJob,
  shadow_review: shadowReview,
  notification,
  review_case: reviewCase,
  review_resolution: reviewResolution,
  recovery_event: recoveryEvent,
  profile_snapshot: profileSnapshot,
  field_profile: fieldProfile,
  drift_event: driftEvent,
  contract_snapshot: contractSnapshot,
  finding,
  audit_artifact: auditArtifact,
};
