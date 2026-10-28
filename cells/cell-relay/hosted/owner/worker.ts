import type { OwnerBindings } from './bindings.js';
import { digest, json, policy, readBody, record, tokenOwner } from './policy.js';
import { ingressUnavailable } from './telemetry.js';
export { KeylessExecutionOwner } from './owner.js';
export default {
  async fetch(request: Request, env: OwnerBindings): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (url.origin !== env.EXECUTION_ORIGIN) return json(403, 'identity_scope');
      if (env.KEYLESS_ENABLED !== 'true') return json(503, 'identity_unavailable');
      const auto = env.KEYLESS_POLICY_MODE==='auto';
      if (env.KEYLESS_POLICY_MODE && !['manual','auto'].includes(env.KEYLESS_POLICY_MODE)) return json(503,'identity_unavailable');
      const config = auto ? undefined : await policy('KEYLESS_REGISTRATIONS' in env ? env.KEYLESS_REGISTRATIONS : '', Math.floor(Date.now() / 1000));
      if (config && config.issuer !== env.EXECUTION_ORIGIN) return json(503, 'identity_unavailable');
      let registration: string | undefined, routedOwner: string | undefined;
      if (url.pathname === '/v1/token' && request.method === 'POST') {
        let body: string, input: Record<string, unknown>;
        try {
          body = await readBody(request, 32768);
          input = record(JSON.parse(body));
        } catch {
          return json(400, 'invalid_request');
        }
        if (
          input.subject_token_type === 'urn:ietf:params:oauth:token-type:access_token' &&
          typeof input.subject_token === 'string'
        ) {
          let owner: string | undefined;
          try {
            owner = tokenOwner(input.subject_token);
          } catch {
            return json(401, 'unauthorized');
          }
          if (auto) routedOwner = owner;
          else registration = config?.registrations.find((row) => row.owner === owner)?.id;
        } else if (typeof input.registration === 'string') {
          if (auto && input.registration.length>0 && input.registration.length<=256 && !/[\s\u007f/*]/u.test(input.registration)) {
            registration=input.registration; routedOwner=await digest('agindow-owner-v1:'+registration);
          } else registration = config?.registrations.find((row) => row.id === input.registration)?.id;
        }
        request = new Request(request.url, { method: 'POST', headers: request.headers, body });
      } else if (
        (url.pathname === '/v1/guard' && request.method === 'POST') ||
        (/^\/v1\/(audit|outcome)\/[^/]+$/u.test(url.pathname) && request.method === 'GET')
      ) {
        const authorization = request.headers.get('authorization');
        if (!authorization?.startsWith('DPoP ')) return json(401, 'unauthorized');
        let owner: string | undefined;
        try {
          owner = tokenOwner(authorization.slice(5));
        } catch {
          return json(401, 'unauthorized');
        }
        if (auto) routedOwner = owner;
        else registration = config?.registrations.find((row) => row.owner === owner)?.id;
      } else return json(404, 'not_found');
      if (auto) {
        if (!routedOwner) return json(403,'identity_registration');
        return await env.AUTHORITY.getByName(routedOwner).execute(request,registration ?? '');
      }
      const row = config?.registrations.find((row) => row.id === registration);
      if (!row) return json(403, 'identity_registration');
      return await env.AUTHORITY.getByName(row.owner).execute(request, row.id);
    } catch {
      try { ingressUnavailable(); } catch { /* Diagnostic only. */ }
      return json(503, 'identity_unavailable');
    }
  },
};
