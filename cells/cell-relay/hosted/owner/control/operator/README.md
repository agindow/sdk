# Private keyless operator transport

Bearer-gated HTTP shim to the `Administration.inspect` / `submit`. Not mounted
on `guard.agindow.com`. Default `fetch` on the policy Worker remains 404.

`workers_dev` is enabled for bring-up only. Prefer a private custom domain or
Zero Trust before treating this as a standing production admin surface. Never
omit `OPERATOR_TOKEN`. The renewal job must not receive this token.

## Deploy (disabled control until you flip it)
```sh
# From cells/cell-relay — generate a long random bearer (keep offline):
python3 -c 'import secrets; print(secrets.token_urlsafe(48))' > /tmp/agindow-operator-token.txt
chmod 600 /tmp/agindow-operator-token.txt

printf '%s' "$(cat /tmp/agindow-operator-token.txt)" | pnpm exec wrangler secret put OPERATOR_TOKEN \
  --config hosted/owner/control/operator/wrangler.jsonc

# First deploy still needs --secrets-file if the Worker does not exist yet:
python3 - <<'PY'
from pathlib import Path
token = Path('/tmp/agindow-operator-token.txt').read_text().strip()
Path('/tmp/agindow-operator-secrets.json').write_text(
    '{"OPERATOR_TOKEN":%s}\n' % __import__('json').dumps(token)
)
Path('/tmp/agindow-operator-secrets.json').chmod(0o600)
print('wrote /tmp/agindow-operator-secrets.json')
PY

pnpm exec wrangler deploy --config hosted/owner/control/operator/wrangler.jsonc \
  --secrets-file /tmp/agindow-operator-secrets.json
```
Note the `*.workers.dev` URL from deploy output.

Enable control before inspect works: set `KEYLESS_CONTROL_ENABLED=true` in
`../wrangler.jsonc` and redeploy `agindow-keyless-policy`.
```sh
TOKEN=$(cat /tmp/agindow-operator-token.txt)
BASE='https://agindow-keyless-operator.<account>.workers.dev'

curl -sS -H "Authorization: Bearer $TOKEN" "$BASE/inspect"
# after sign-review.mjs:
curl -sS -X POST -H "Authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  --data-binary @command.json \
  "$BASE/submit"
```