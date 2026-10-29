import { constants, closeSync, fstatSync, lstatSync, openSync, readSync, realpathSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { isAbsolute, join, parse, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import type { CheckoutGrant, RefusalReason } from './types.js';
export const MAX_SOURCE_BYTES = 1024 * 1024;
export const digest = (bytes: string | Buffer): string => createHash('sha256').update(bytes).digest('hex');
const excluded = /^(?:node_modules|vendor|vendors|generated|__generated__|\.git|target|dist|build|coverage|\.next|\.turbo)$/i;
export function safePath(file: string): boolean {
  const parts = file.split('/');
  return file.length <= 1024 && !file.includes('\\') && !file.includes('\0') && !isAbsolute(file)
    && parts.length <= 64 && parts.every(part => part !== '' && part !== '.' && part !== '..' && !excluded.test(part))
    && /\.[cm]?[jt]sx?$/.test(file) && !/(?:\.generated\.|\.min\.|\.d\.[cm]?ts$)/i.test(file);
}
/** Reject symlinks in every component, including the root's ancestors. */
function noLinks(path: string): void {
  const absolute = resolve(path); let current = parse(absolute).root;
  for (const part of absolute.slice(current.length).split('/').filter(Boolean)) {
    current = join(current, part);
    if (lstatSync(current).isSymbolicLink()) throw Error('symlink');
  }
}
function git(root: string, args: string[], maxBuffer = 4096): Buffer {
  // Fixed read-only plumbing commands: no shell, hooks, filters or user scripts.
  return execFileSync('git', ['--no-optional-locks', '-C', root, ...args], {
    maxBuffer, timeout: 5000, stdio: ['ignore', 'pipe', 'pipe'],
    env: Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_'))),
  });
}
export type SourceRead = { readonly source: string; readonly fileDigest: string } | { readonly reason: RefusalReason };
export function readSource(grant: CheckoutGrant, file: string): SourceRead {
  if (!safePath(file)) return { reason: 'unsafe_path' };
  try { noLinks(grant.root); noLinks(join(grant.root, file)); } catch { return { reason: 'unsafe_path' }; }
  try {
    if (!isAbsolute(grant.root) || realpathSync(grant.root) !== resolve(grant.root)
      || git(grant.root, ['rev-parse', '--show-toplevel']).toString().trim() !== resolve(grant.root)) return { reason: 'unsafe_path' };
    if (git(grant.root, ['rev-parse', '--verify', 'HEAD']).toString().trim() !== grant.baseSha) return { reason: 'stale_base' };
    const path = join(grant.root, file);
    if (!lstatSync(path).isFile()) return { reason: 'unsafe_path' };
    const descriptor = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    let bytes: Buffer;
    try {
      const stat = fstatSync(descriptor);
      if (!stat.isFile()) return { reason: 'unsafe_path' };
      if (stat.size > MAX_SOURCE_BYTES) return { reason: 'source_limit' };
      const buffer = Buffer.alloc(MAX_SOURCE_BYTES + 1);
      let size = 0;
      while (size < buffer.length) {
        const read = readSync(descriptor, buffer, size, buffer.length - size, size);
        if (read === 0) break;
        size += read;
      }
      if (size > MAX_SOURCE_BYTES) return { reason: 'source_limit' };
      bytes = buffer.subarray(0, size);
    } finally { closeSync(descriptor); }
    // cat-file returns committed blob bytes, without checkout filters or textconv.
    const committed = git(grant.root, ['cat-file', 'blob', `${grant.baseSha}:${file}`], MAX_SOURCE_BYTES + 1);
    if (!bytes.equals(committed)) return { reason: 'stale_source' };
    const source = bytes.toString('utf8');
    if (!Buffer.from(source).equals(bytes)) return { reason: 'unsupported_syntax' };
    if (/@generated|@auto-generated|automatically generated|do not edit/i.test(source)) return { reason: 'excluded_source' };
    return { source, fileDigest: digest(bytes) };
  } catch { return { reason: 'unavailable' }; }
}
