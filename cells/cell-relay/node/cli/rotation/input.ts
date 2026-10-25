import { createReadStream } from 'node:fs';
/** Fixed chunk and line bounds; readline alone would buffer an unbounded line. */
export async function* jsonLines(path: string, lineCap = 8192): AsyncGenerator<unknown> {
  let pending = Buffer.alloc(0);
  for await (const raw of createReadStream(path, { highWaterMark: 8192 })) {
    const chunk: Buffer = Buffer.isBuffer(raw) ? raw : Buffer.from(String(raw));
    let start = 0;
    for (let end = 0; end < chunk.length; end++) {
      if (chunk[end] !== 10) continue;
      if (pending.length + end - start > lineCap) throw new Error('Rotation input line too large');
      const line = Buffer.concat([pending, chunk.subarray(start, end)]).toString('utf8');
      pending = Buffer.alloc(0); start = end + 1;
      if (line.trim().length > 0) yield JSON.parse(line) as unknown;
    }
    pending = Buffer.concat([pending, chunk.subarray(start)]);
    if (pending.length > lineCap) throw new Error('Rotation input line too large');
  }
  if (pending.length > 0) yield JSON.parse(pending.toString('utf8')) as unknown;
}
