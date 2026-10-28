import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';

async function load() {
  const result = await build({
    entryPoints: [new URL('../owner/control/operator/handler.ts', import.meta.url).pathname],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'esm',
    target: 'es2022',
    logLevel: 'silent',
  });
  const output = result.outputFiles?.[0];
  assert.ok(output);
  return /** @type {typeof import('../owner/control/operator/handler.ts')} */ (
    await import('data:text/javascript;base64,' + Buffer.from(output.text).toString('base64'))
  );
}

test('operator transport refuses missing or wrong bearer before Administration RPC', async () => {
  const { handleOperator } = await load();
  let calls = 0;
  const env = {
    OPERATOR_TOKEN: 'a'.repeat(48),
    ADMIN: {
      async inspect() {
        calls += 1;
        return '{}';
      },
      async submit() {
        calls += 1;
        return '{}';
      },
    },
  };
  const denied = await handleOperator(new Request('https://op.invalid/inspect'), env);
  assert.equal(denied.status, 401);
  assert.equal(calls, 0);
  const wrong = await handleOperator(
    new Request('https://op.invalid/inspect', { headers: { authorization: 'Bearer ' + 'b'.repeat(48) } }),
    env,
  );
  assert.equal(wrong.status, 401);
  assert.equal(calls, 0);
});

test('operator transport forwards inspect and submit, and 404s other routes', async () => {
  const { handleOperator } = await load();
  const env = {
    OPERATOR_TOKEN: 'c'.repeat(48),
    ADMIN: {
      async inspect() {
        return '{"generation":"g"}';
      },
      /** @param {string} command */
      async submit(command) {
        assert.equal(command, '{"action":"admit"}');
        return '{"revision":1}';
      },
    },
  };
  const headers = { authorization: 'Bearer ' + 'c'.repeat(48) };
  const inspect = await handleOperator(new Request('https://op.invalid/inspect', { headers }), env);
  assert.equal(inspect.status, 200);
  assert.equal(await inspect.text(), '{"generation":"g"}');
  const submit = await handleOperator(
    new Request('https://op.invalid/submit', { method: 'POST', headers, body: '{"action":"admit"}' }),
    env,
  );
  assert.equal(submit.status, 200);
  assert.equal(await submit.text(), '{"revision":1}');
  const missing = await handleOperator(new Request('https://op.invalid/other', { headers }), env);
  assert.equal(missing.status, 404);
});
