use super::{
    Actor, Confirmation, Context, Denial, Exchange, KeyCache, ProofRequest, Replay, sign,
    verify_possession, wire::tools_valid,
};
pub struct DelegateRequest<'a> {
    pub parent: &'a str,
    pub context: Context<'a>,
    pub proof: Option<&'a str>,
    pub tls_peer: Option<&'a str>,
    pub target: &'a str,
    pub tools: Vec<String>,
    pub exp: u64,
    pub subject: &'a str,
    pub cnf: Confirmation,
    pub max_depth: usize,
}
impl Exchange {
    /// The route context is trusted host state; it fixes tenant, environment,
    /// connection and audience. The caller can narrow tools/expiry only.
    pub fn delegate(
        &mut self,
        req: &DelegateRequest<'_>,
        keys: &mut KeyCache,
        replay: &mut Replay,
        now: u64,
    ) -> Result<String, Denial> {
        let mut parent = keys.validate(req.parent, &req.context, now)?;
        if !(1..=8).contains(&req.max_depth) || parent.act.len() >= req.max_depth {
            self.limited += 1;
            return Err(Denial::Capacity);
        }
        if req.exp <= now || req.exp > parent.exp {
            return Err(Denial::Expiry);
        }
        if !tools_valid(&req.tools)
            || !req.tools.iter().all(|t| parent.tools.contains(t))
            || (req.exp == parent.exp && req.tools.len() == parent.tools.len())
        {
            return Err(Denial::Scope);
        }
        verify_possession(
            &parent,
            &ProofRequest {
                method: "POST",
                target: req.target,
                token: req.parent,
            },
            req.proof,
            req.tls_peer,
            replay,
            now,
        )?;
        self.rate(&format!("{}:{}", parent.tenant, parent.sub), now)?;
        parent.act.push(Actor {
            jti: parent.jti,
            sub: parent.sub,
            tenant: parent.tenant.clone(),
            environment: parent.environment.clone(),
            aud: parent.aud.clone(),
            connection: parent.connection.clone(),
            tools: parent.tools,
            exp: parent.exp,
        });
        parent.jti = self.next_id(now)?;
        parent.sub = req.subject.into();
        parent.tools = req.tools.clone();
        parent.exp = req.exp;
        parent.iat = now;
        parent.nbf = now;
        parent.cnf = req.cnf.clone();
        let ctx = Context {
            tool: parent.tools.first().ok_or(Denial::Scope)?,
            ..req.context
        };
        parent.check(&ctx, now)?;
        sign(&parent, &self.kid, &self.key)
    }
}
