import { readdirSync, readFileSync, lstatSync } from 'node:fs';
import { join, relative } from 'node:path';
import { analyze } from './analyze.js';
import type { DetectedTool, ScanOptions } from './analyze.js';

export type { DetectedTool, ScanOptions, SourceFile } from './analyze.js';
export { analyze, scanSources } from './analyze.js';

const excluded = new Set(['node_modules', '.git', 'target', 'dist', 'build', '.next', '.turbo']);

/** Directory walk for CLI report. Browser callers use scanSources instead. */
export function scan(directory: string, options: ScanOptions = {}): { tools: DetectedTool[]; skipped: number } {
  const tools: DetectedTool[] = [];
  let skipped = 0;
  let files = 0;
  function walk(path: string, depth: number): void {
    if (depth > 64) {
      skipped++;
      return;
    }
    let entries: string[];
    try {
      entries = readdirSync(path).sort();
    } catch {
      skipped++;
      return;
    }
    for (const entry of entries) {
      const full = join(path, String(entry));
      try {
        const stat = lstatSync(full);
        if (stat.isSymbolicLink()) {
          skipped++;
          continue;
        }
        if (stat.isDirectory()) {
          if (!excluded.has(String(entry))) walk(full, depth + 1);
          continue;
        }
        if (!/\.[cm]?[jt]sx?$/.test(String(entry))) continue;
        if (++files > (options.maxFiles ?? 10_000) || stat.size > (options.maxBytes ?? 1024 * 1024)) {
          skipped++;
          continue;
        }
        tools.push(...analyze(readFileSync(full, 'utf8'), relative(directory, full)));
      } catch {
        skipped++;
      }
    }
  }
  walk(directory, 0);
  return { tools, skipped };
}
