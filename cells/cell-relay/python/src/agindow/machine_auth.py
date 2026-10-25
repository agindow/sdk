"""In-memory keyless sessions; identity acquisition runs outside guarded calls."""
from __future__ import annotations
from base64 import urlsafe_b64encode
from dataclasses import dataclass
from hashlib import sha256
import json
import math
import re
from threading import Lock, Timer
from time import time
from typing import Callable
from urllib.parse import urlsplit, urlunsplit
from uuid import uuid4
from .types import AgindowSetupError

CLOCK_SKEW_SECONDS = 30
MAX_TOKEN_TTL_SECONDS = 900

def _b64(value: bytes) -> str:
    return urlsafe_b64encode(value).rstrip(b'=').decode('ascii')

def canonical_target(target: str) -> str:
    url = urlsplit(target)
    if url.scheme not in ('http', 'https') or not url.hostname or url.username or url.password:
        raise AgindowSetupError('invalid proof target')
    host = url.hostname.lower()
    if ':' in host:
        host = '[' + host + ']'
    port = url.port
    if port is not None and port != (443 if url.scheme == 'https' else 80):
        host += ':' + str(port)
    return urlunsplit((url.scheme, host, url.path or '/', '', ''))

@dataclass(frozen=True, repr=False)
class AccessCredential:
    access_token: str
    expires_at: int

class ProofSigner:
    """Host-owned Ed25519 signer. The callback holds private-key custody.

    public_x is the base64url public JWK coordinate; sign receives JWT bytes
    and returns a 64-byte Ed25519 signature. Neither callback nor key is emitted.
    """
    __slots__ = ('__x', '__sign', 'thumbprint')
    def __init__(self, public_x: str, sign: Callable[[bytes], bytes]) -> None:
        if not re.fullmatch(r'[A-Za-z0-9_-]{43}', public_x):
            raise AgindowSetupError('invalid public proof key')
        self.__x = public_x
        self.__sign = sign
        canonical = json.dumps({'crv': 'Ed25519', 'kty': 'OKP', 'x': public_x}, separators=(',', ':'))
        self.thumbprint = _b64(sha256(canonical.encode()).digest())

    def proof(self, token: str, method: str, target: str, now: float) -> str:
        header = {'alg': 'EdDSA', 'typ': 'dpop+jwt', 'jwk': {'kty': 'OKP', 'crv': 'Ed25519', 'x': self.__x}}
        claims = {'jti': str(uuid4()), 'htm': method, 'htu': canonical_target(target),
                  'iat': math.floor(now), 'ath': _b64(sha256(token.encode()).digest())}
        signed = '.'.join(_b64(json.dumps(v, separators=(',', ':')).encode()) for v in (header, claims))
        return signed + '.' + _b64(self.__sign(signed.encode()))

def valid_legacy_key(value: str) -> bool:
    return re.fullmatch(r'agw_(live|test)_[A-Za-z0-9_-]{32,256}', value) is not None

class RemoteAuth:
    """Explicit setup/background exchange; headers only read memory and sign.

    acquire must exchange a platform assertion bound to the supplied thumbprint.
    Tenant authority comes from that exchange, never local tenant configuration.
    close stops refresh and releases cached credentials. Exceptions are redacted.
    """
    def __init__(self, acquire: Callable[[str], AccessCredential], signer: ProofSigner, *,
                 now: Callable[[], float] = time, schedule: bool = True,
                 allow_legacy_fallback: bool = False, legacy_key: str | None = None,
                 on_diagnostic: Callable[[str], None] | None = None) -> None:
        if legacy_key is not None and not valid_legacy_key(legacy_key):
            raise AgindowSetupError('invalid legacy key')
        self.__acquire = acquire
        self.__signer = signer
        self.__now = now
        self.__schedule = schedule
        self.__diagnostic = on_diagnostic
        self.__credential: AccessCredential | None = None
        self.__legacy: str | None = None
        self.__timer: Timer | None = None
        self.__closed = False
        self.__lock = Lock()
        if not self.refresh():
            if allow_legacy_fallback and legacy_key is not None:
                self.__legacy = legacy_key
                self._emit('legacy_unbound')
            else:
                self.close()
                raise AgindowSetupError('keyless identity unavailable') from None

    def _emit(self, reason: str) -> None:
        try:
            if self.__diagnostic is not None:
                self.__diagnostic(reason)
        except Exception:
            pass

    def _schedule(self, delay: float) -> None:
        if self.__closed or not self.__schedule:
            return
        if self.__timer is not None:
            self.__timer.cancel()
        self.__timer = Timer(max(1, delay), self.refresh)
        self.__timer.daemon = True
        self.__timer.start()

    def refresh(self) -> bool:
        if self.__closed or not self.__lock.acquire(blocking=False):
            return False
        try:
            c = self.__acquire(self.__signer.thumbprint)
            now = self.__now()
            if not math.isfinite(now) or type(c.expires_at) is not int or not 0 < c.expires_at - now <= MAX_TOKEN_TTL_SECONDS or not c.access_token or len(c.access_token) > 16384 or re.search(r'[\s\x7f]', c.access_token):
                raise ValueError('invalid exchange result')
            if self.__closed:
                return False
            self.__credential = c
            self.__legacy = None
            self._emit('keyless_ready')
            self._schedule(c.expires_at - now - 60)
            return True
        except Exception:
            self._emit('refresh_unavailable')
            self._schedule(5)
            return False
        finally:
            self.__lock.release()

    def available(self) -> bool:
        c = self.__credential
        return not self.__closed and ((c is not None and math.isfinite(self.__now()) and self.__now() < c.expires_at) or self.__legacy is not None)

    def headers(self, method: str, target: str) -> dict[str, str] | None:
        if self.__closed:
            return None
        c = self.__credential
        now = self.__now()
        if c is not None and math.isfinite(now) and now < c.expires_at:
            try:
                return {'authorization': 'DPoP ' + c.access_token,
                        'dpop': self.__signer.proof(c.access_token, method, target, now)}
            except Exception:
                return None
        return {'authorization': 'Bearer ' + self.__legacy} if self.__legacy is not None else None

    def close(self) -> None:
        self.__closed = True
        if self.__timer is not None:
            self.__timer.cancel()
        self.__credential = None
        self.__legacy = None
