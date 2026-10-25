"""Background workload assertion file/host-source exchange, never call-path I/O."""
from __future__ import annotations
from os import environ
from pathlib import Path
import json
from typing import Callable
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener
from .machine_auth import AccessCredential
from .types import AgindowSetupError

class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req: Request, fp: object, code: int, msg: str,
                         headers: object, newurl: str) -> None:
        return None

def workload_identity(*, exchange_endpoint: str, registration: str, tools: tuple[str, ...],
                      audience: str = 'agindow:guard', ttl: int = 300,
                      token_file: str | None = None,
                      assertion: Callable[[], str] | None = None) -> Callable[[str], AccessCredential]:
    """Prepare a callback for RemoteAuth. Only the registration grants tenant access."""
    url = urlsplit(exchange_endpoint)
    if url.scheme != 'https' or not url.hostname or url.username or url.password or url.fragment:
        raise AgindowSetupError('invalid identity exchange endpoint')
    file = token_file or environ.get('AGINDOW_IDENTITY_TOKEN_FILE')
    def acquire(thumbprint: str) -> AccessCredential:
        if assertion is not None:
            value = assertion()
        else:
            if file is None:
                raise AgindowSetupError('workload identity unavailable')
            with Path(file).open('rb') as stream:
                raw = stream.read(16385)
            if len(raw) > 16384:
                raise ValueError('identity size')
            value = raw.decode().strip()
        if not value or len(value) > 16384 or any(c.isspace() or ord(c) == 127 for c in value):
            raise ValueError('invalid identity assertion')
        payload = {'grant_type': 'urn:ietf:params:oauth:grant-type:token-exchange',
                   'subject_token_type': 'urn:ietf:params:oauth:token-type:jwt',
                   'subject_token': value, 'registration': registration, 'audience': audience,
                   'tools': tools, 'ttl': ttl, 'cnf': {'jkt': thumbprint}}
        request = Request(exchange_endpoint, data=json.dumps(payload).encode(),
                          headers={'content-type': 'application/json'}, method='POST')
        opener = build_opener(_NoRedirect())
        with opener.open(request, timeout=5) as response:
            raw = response.read(20001)
        if len(raw) > 20000:
            raise ValueError('exchange size')
        result: object = json.loads(raw)
        if not isinstance(result, dict) or not isinstance(result.get('access_token'), str) or type(result.get('expires_at')) is not int or result.get('token_type') != 'DPoP':
            raise ValueError('invalid exchange result')
        return AccessCredential(result['access_token'], result['expires_at'])
    return acquire
