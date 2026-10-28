import { authorize } from './auth.js';

export interface AdministrationBinding {
  inspect(): Promise<string>;
  submit(command: string): Promise<string>;
}

export interface OperatorEnv {
  readonly ADMIN: AdministrationBinding;
  readonly OPERATOR_TOKEN: string;
}

/** Route private operator HTTP to Administration RPC after bearer checks. */
export async function handleOperator(request: Request, env: OperatorEnv): Promise<Response> {
  if (!authorize(request.headers.get('authorization'), env.OPERATOR_TOKEN)) {
    return Response.json({ error: 'unauthorized' }, { status: 401, headers: { 'cache-control': 'no-store' } });
  }
  const url = new URL(request.url);
  try {
    if (url.pathname === '/inspect' && (request.method === 'GET' || request.method === 'POST')) {
      return text(await env.ADMIN.inspect());
    }
    if (url.pathname === '/submit' && request.method === 'POST') {
      const body = await request.text();
      if (body.length === 0 || body.length > 300000) {
        return Response.json({ error: 'invalid_request' }, { status: 400, headers: { 'cache-control': 'no-store' } });
      }
      return text(await env.ADMIN.submit(body));
    }
  } catch {
    return Response.json({ error: 'authority_unavailable' }, { status: 503, headers: { 'cache-control': 'no-store' } });
  }
  return Response.json({ error: 'not_found' }, { status: 404, headers: { 'cache-control': 'no-store' } });
}

function text(body: string): Response {
  return new Response(body, {
    status: 200,
    headers: {
      'content-type': 'application/json; charset=utf-8',
      'cache-control': 'no-store',
      'x-content-type-options': 'nosniff',
    },
  });
}
