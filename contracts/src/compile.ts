/** Default Node compiler. Worker builds replace only this module with static validators. */
import Ajv2020, { type ValidateFunction } from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import { CONTRACTS, type ContractName } from './schemas.js';

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
const compiled = new Map<ContractName, ValidateFunction>();
export function validatorFor(contract: ContractName): ValidateFunction {
  const existing = compiled.get(contract);
  if (existing !== undefined) return existing;
  const validate = ajv.compile(CONTRACTS[contract]);
  compiled.set(contract, validate);
  return validate;
}
