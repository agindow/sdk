import type { DocSection } from './model.js';
const object = (value: unknown): Readonly<Record<string, unknown>> | undefined =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? value as Readonly<Record<string, unknown>> : undefined;
/** Tables derive from the original schema; full JSON retains nested constraints. */
export function operationSections(value: unknown): readonly DocSection[] {
  const operation = object(value);
  if (!operation) throw Error('Invalid OpenAPI operation');
  const request = object(operation['requestBody']);
  const content = object(request?.['content']);
  const json = object(content?.['application/json']);
  const schema = object(json?.['schema']);
  const properties = object(schema?.['properties']);
  const required: readonly unknown[] = Array.isArray(schema?.['required']) ? schema['required'] : [];
  const responses = object(operation['responses']);
  return [
    { heading: 'Published operation details', text: [operation['summary'], operation['description']].filter(item => typeof item === 'string').join('\n') || 'Published operation definition.', schema: JSON.stringify(operation, null, 2) },
    ...(properties ? [{ heading: 'Request fields', text: 'JSON properties and constraints projected from requestBody.content.application/json.schema.', table: {
      columns: ['Field', 'Schema', 'Presence'], rows: Object.entries(properties).map(([name, definition]) => [name, JSON.stringify(definition), required.includes(name) ? 'Required' : 'Optional']),
    } }] : []),
    ...(responses ? [{ heading: 'Responses', text: 'Status-specific responses from the published operation.', table: {
      columns: ['HTTP', 'Definition'], rows: Object.entries(responses).map(([status, definition]) => [status, JSON.stringify(definition)]),
    } }] : []),
  ];
}
