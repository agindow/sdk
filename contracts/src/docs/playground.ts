/** Reviewed synthetic differentiation fixtures; shared by browser/publication gates. */
import rename from './fixtures/playground-rename.json';
export const playgroundRename = Object.freeze(rename);
/** JSON Schema draft-07: positive numeric amounts pass before AND after magnitude drift. */
export const paymentSchema = {
  $schema: 'http://json-schema.org/draft-07/schema#',
  type: 'object', required: ['value'],
  properties: { value: { type: 'number', minimum: 0 } },
  additionalProperties: false,
} as const;
