/**
 * TII-28 — the closed `notification` v1 semantic boundary (contract §10.39).
 *
 * The schema carries the envelope; this carries the promises the schema cannot
 * express: identifiers are byte-bounded, timestamps are explicit UTC and
 * ordered, the consolidation identity is coherent, and no reference may smuggle
 * credential-shaped content or payload bytes. Nothing here queues or delivers.
 */

import type { Notification } from './generated/notification.js';
import { copyBounded } from './bounds.js';

/** One serialized envelope, including JSON framing. */
export const NOTIFICATION_BYTES = 8 * 1024;
/** Bounded audit references per logical notification. */
export const NOTIFICATION_REFS = 32;
/** One identifier's UTF-8 byte ceiling. */
export const NOTIFICATION_IDENTIFIER_BYTES = 256;

const BOUNDS = { bytes: NOTIFICATION_BYTES, depth: 8, nodes: 256 };
const encoder = new TextEncoder();

const CREDENTIAL = [
  /^bearer\s/i,
  /^basic\s/i,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/i,
  /\bsk_(?:live|test)_[A-Za-z0-9]{8,}/,
  /\bAKIA[0-9A-Z]{16}\b/,
  /\bgh[pousr]_[A-Za-z0-9]{20,}/,
  /\bxox[baprs]-[A-Za-z0-9-]{10,}/,
  /^eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/,
  /(?:api[_-]?key|token|secret|password|passwd|credential|authorization)\s*[:=]/i,
] as const;

/** Replacement bytes and encoded payload bodies share these shapes. */
const PAYLOAD = [
  /^(?:[0-9a-fA-F]{2}){32,}$/,
  /^[{[]/,
  /^data:[a-z]+\//i,
] as const;

const CONTROL = /[\u0000-\u001f\u007f]/u;

/** True when a string claims to be a secret or an encoded payload body. */
export function suspiciousContent(value: string): boolean {
  return CREDENTIAL.some((pattern) => pattern.test(value)) || PAYLOAD.some((pattern) => pattern.test(value));
}

/** One bounded identifier: 1–256 UTF-8 bytes, no control characters. */
export function notificationText(value: string, limit = NOTIFICATION_IDENTIFIER_BYTES): string {
  if (encoder.encode(value).length < 1 || encoder.encode(value).length > limit || CONTROL.test(value)) {
    throw new Error('invalid notification identifier');
  }
  if (suspiciousContent(value)) throw new Error('credential-shaped or payload content');
  return value;
}

function utc(value: string, label: string): number {
  if (!value.endsWith('Z') || !Number.isFinite(Date.parse(value))) throw new Error(`invalid notification ${label}`);
  return Date.parse(value);
}

function scan(value: unknown): void {
  if (typeof value === 'string') {
    if (suspiciousContent(value)) throw new Error('credential-shaped or payload content');
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) scan(item);
    return;
  }
  if (typeof value === 'object' && value !== null) {
    for (const item of Object.values(value)) scan(item);
  }
}

/**
 * Cross-field rules over the structurally valid envelope. Invalid or
 * inconsistent evidence fails whole; nothing is repaired or truncated.
 */
export function checkNotification(value: Notification): void {
  if (!/^ntf_[0-9a-f]{32}$/u.test(value.id)) throw new Error('invalid notification id');
  notificationText(value.id);
  notificationText(value.tenant);
  notificationText(value.scope.connection);
  notificationText(value.scope.vendor);
  notificationText(value.scope.operation);
  const emitted = utc(value.ts, 'time');
  const first = utc(value.occurrences.first_seen, 'first-seen time');
  const last = utc(value.occurrences.last_seen, 'last-seen time');
  if (first > last || last > emitted) throw new Error('notification times are not ordered');
  if (value.audit_refs.length > NOTIFICATION_REFS) throw new Error('too many notification references');
  for (const reference of value.audit_refs) notificationText(reference);
  if (value.occurrences.count < value.consolidation.count) throw new Error('summary exceeds occurrences');
  if (value.consolidation.mode === 'individual') {
    if (value.consolidation.count !== 1) throw new Error('an individual notification consolidates exactly one finding');
    if (value.audit_refs.length === 0) throw new Error('an individual notification requires audit evidence');
  }
  scan({
    severity: value.severity,
    finding: value.finding,
    consolidation: value.consolidation,
    occurrences: value.occurrences,
  });
}

/** Detach and bound one envelope before structural validation. */
export function copyNotification(value: unknown): unknown {
  return copyBounded(value, BOUNDS);
}
