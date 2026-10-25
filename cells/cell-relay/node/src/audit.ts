import { parseAuditArtifact } from 'contracts';
/** A frozen JSON artifact, validated against audit_artifact v1 at the boundary. */
export interface AuditRecord extends Readonly<Record<string, unknown>> { readonly id: string }
export type AuditSink = (artifact: AuditRecord) => void | Promise<void>;
export interface AuditDrain { readonly delivered: number; readonly pending: number; readonly dropped: number; readonly available: boolean }
export function validateAudit(value: unknown): AuditRecord {
  const parsed = parseAuditArtifact(value);
  return Object.freeze(parsed) as AuditRecord;
}
export const unavailableAudit: AuditDrain = Object.freeze({ delivered: 0, pending: 0, dropped: 0, available: false });
