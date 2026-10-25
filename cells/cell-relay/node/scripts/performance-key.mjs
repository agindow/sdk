import { createHash } from 'node:crypto';
import { performanceEnvironment } from './performance-environment.mjs';
console.log('key=' + createHash('sha256').update(JSON.stringify(performanceEnvironment())).digest('hex'));
