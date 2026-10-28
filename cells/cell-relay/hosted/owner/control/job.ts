export default {
  fetch(): Response { return new Response(null,{status:404}); },
  async scheduled(_event: ScheduledController, env: KeylessRenewalBindings): Promise<void> {
    if (env.KEYLESS_RENEWAL_ENABLED!=='true') return;
    try { await env.RENEWAL.renewCurrent(); }
    catch { console.info(JSON.stringify({event:'policy_job',outcome:'unavailable'})); }
  },
};
