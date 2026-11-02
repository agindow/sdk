/** Independent host oracle: fixed-scale BigInt division, no floating point. */
const pattern = /^(-?)(0|[1-9][0-9]*)(?:\.([0-9]+))?(?:[eE]([+-]?[0-9]+))?$/u;
const LIMIT = 10n ** 36n;
export function decimal(value: string): bigint {
  if (value.length > 80) throw Error('decimal bound');
  const match = pattern.exec(value); if (!match) throw Error('not a numeric token');
  const [, sign, integer, fraction = '', exponent = '0'] = match;
  const exp = Number(exponent);
  if (!Number.isInteger(exp) || Math.abs(exp) > 18 || (integer ?? '').length + fraction.length > 36) throw Error('decimal precision');
  const power = 18 + exp - fraction.length;
  let result = BigInt(`${sign ?? ''}${integer ?? ''}${fraction}`);
  if (power < 0) {
    const divisor = 10n ** BigInt(-power); if (result % divisor !== 0n) throw Error('decimal precision'); result /= divisor;
  } else result *= 10n ** BigInt(power);
  // Coefficient bound after removing trailing decimal zeros, as in native.
  let coefficient = result; let places = 18;
  while (places > 0 && coefficient % 10n === 0n) { coefficient /= 10n; places--; }
  if (coefficient <= -LIMIT || coefficient >= LIMIT) throw Error('decimal overflow');
  return result;
}
export function scaled(value: string, numerator: string, denominator: string): string {
  const n = decimal(value) * BigInt(numerator); const d = BigInt(denominator);
  if (d <= 0n || n % d !== 0n) throw Error('inexact scale');
  const output = n / d;
  const sign = output < 0n ? '-' : '';
  const digits = (output < 0n ? -output : output).toString().padStart(19, '0');
  const text = `${sign}${digits.slice(0, -18)}.${digits.slice(-18)}`.replace(/\.?0+$/u, '');
  decimal(text); return text;
}
