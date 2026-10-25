"""Explicit remote HTTP execution; local decorators remain synchronous and offline."""
from __future__ import annotations
from dataclasses import dataclass
from http.client import HTTPConnection, HTTPSConnection, HTTPResponse
from time import monotonic
from typing import Callable, Literal, TypeVar, cast
from urllib.parse import urlsplit, urljoin
import asyncio
import json
import math
from .machine_auth import RemoteAuth, valid_legacy_key
from .types import AgindowDriftError, AgindowSetupError, Event, Mode, Operation, Phase, PolicyFeedback, Verdict

R = TypeVar("R")

@dataclass(frozen=True)
class RemoteEnvelope:
    tool: str
    connection: str
    operation: Operation
    args: object
    mode: Mode
    idempotency_key: str | None = None

@dataclass(frozen=True)
class RemoteOptions:
    endpoint: str
    api_key: str | None = None
    timeout_seconds: float = 5
    on_event: Callable[[Event], None] | None = None
    auth: RemoteAuth | None = None
    on_auth_diagnostic: Callable[[str], None] | None = None

@dataclass(frozen=True)
class RemoteResponse:
    status: int
    headers: dict[str, str]
    body: HTTPResponse | bytes

    def read(self, amount: int | None = None) -> bytes:
        if isinstance(self.body, bytes):
            return self.body if amount is None else self.body[:amount]
        return self.body.read(amount)

    def close(self) -> None:
        if isinstance(self.body, HTTPResponse):
            self.body.close()


def _send(endpoint: str, options: RemoteOptions, payload: bytes | None) -> tuple[int, dict[str, str], HTTPResponse | bytes]:
    url = urlsplit(endpoint)
    connection = (HTTPSConnection if url.scheme == "https" else HTTPConnection)(url.hostname or "", url.port, timeout=options.timeout_seconds)
    headers = {"content-type": "application/json"}
    if options.auth is not None:
        auth_headers = options.auth.headers("POST" if payload is not None else "GET", endpoint)
        if auth_headers is None:
            raise ConnectionError("auth unavailable")
        headers.update(auth_headers)
    if options.api_key is not None:
        headers["authorization"] = f"Bearer {options.api_key}"
    try:
        connection.request("POST" if payload is not None else "GET", url.path + (f"?{url.query}" if url.query else ""), payload, headers)
        response = connection.getresponse()
        # HTTPResponse owns the socket until read/close; never buffer success bodies.
        return response.status, {key.lower(): value for key, value in response.getheaders()}, response
    except Exception:
        connection.close()
        raise


def _transport(input: RemoteEnvelope, options: RemoteOptions) -> RemoteResponse | None:
    if options.auth is not None and options.api_key is not None:
        raise AgindowSetupError("configure legacy fallback in the auth session")
    if options.api_key is not None:
        if not valid_legacy_key(options.api_key):
            raise AgindowSetupError("invalid legacy key")
        try:
            if options.on_auth_diagnostic is not None:
                options.on_auth_diagnostic("legacy_unbound")
        except Exception:
            pass
    url = urlsplit(options.endpoint)
    if url.scheme not in ("http", "https") or not url.hostname or url.username or url.password or url.fragment or not math.isfinite(options.timeout_seconds) or options.timeout_seconds <= 0:
        raise AgindowSetupError("Invalid remote endpoint/timeout")
    if input.operation not in ("read", "write", "unknown") or input.mode not in ("observe", "repair", "strict") or not input.tool or not input.connection:
        raise AgindowSetupError("Invalid remote envelope")
    if input.operation != "read" and not input.idempotency_key:
        raise AgindowSetupError("Remote writes require the same upstream idempotency key")
    if options.auth is not None and not options.auth.available():
        return None
    started = monotonic()
    try:
        wire: dict[str, object] = {"tool": input.tool, "connection": input.connection, "operation": input.operation, "args": input.args, "mode": input.mode}
        if input.idempotency_key is not None:
            wire["idempotency_key"] = input.idempotency_key
        status, headers, body = _send(options.endpoint, options, json.dumps(wire, allow_nan=False).encode())
        response = RemoteResponse(status, headers, body)
        if status >= 500 and headers.get("x-agindow-status") in ("ok", "degraded", "repaired", "bypassed"):
            return response
        if status >= 500:
            response.close()
            return None
        if status != 409 or headers.get("x-agindow-status") not in (None, "blocked"):
            return response
        try:
            value: object = json.loads(response.read(16385))
        finally:
            response.close()
        if not isinstance(value, dict) or set(value) != {"error", "tool", "drift_class", "drift_id", "hint", "audit"} or not all(isinstance(item, str) for item in value.values()):
            return None
        fields = cast(dict[str, str], value)
        code = "drift_halted" if input.mode == "strict" else "drift_blocked"
        if input.mode == "observe" or fields["error"] != code or fields["tool"] != input.tool or fields["drift_class"] not in ("shape", "semantic", "unknown") or not fields["drift_id"] or not fields["audit"]:
            return None
        feedback = PolicyFeedback(cast(Literal["drift_halted", "drift_blocked"], code), input.tool, cast(Literal["shape", "semantic", "unknown"], fields["drift_class"]), fields["drift_id"], fields["hint"], fields["audit"])
    except Exception:
        return None
    phase: Phase = "response" if "x-agindow-outcome" in headers or headers.get("x-agindow-drift-side") == "response" else "request"
    retained = headers.get("x-agindow-outcome")
    if phase == "response" and retained is not None:
        try:
            location = urljoin(options.endpoint, retained)
            target = urlsplit(location)
            remaining = options.timeout_seconds - (monotonic() - started)
            if target.scheme == url.scheme and target.netloc == url.netloc and target.path.startswith("/v1/outcome/") and remaining > 0:
                recovery = RemoteOptions(location, options.api_key, remaining, auth=options.auth)
                recovered_status, recovered_headers, recovered_body = _send(location, recovery, None)
                recovered = RemoteResponse(recovered_status, recovered_headers, recovered_body)
                if recovered_headers.get("x-agindow-retained-outcome") == "true" or 200 <= recovered_status < 300:
                    response = recovered
                else:
                    recovered.close()
        except Exception:
            pass  # A known completed operation is never replayed after recovery failure.
    raise AgindowDriftError(input.tool, input.mode, phase, Verdict(halt=True, drift_class=feedback.drift_class, findings=1, feedback=feedback), response)


def guard_remote(original: Callable[[RemoteEnvelope], R], input: RemoteEnvelope, options: RemoteOptions) -> RemoteResponse | R:
    started = monotonic()
    response = _transport(input, options)
    if (response is None or response.status >= 500) and options.on_event is not None:
        try:
            options.on_event(Event("bypass", input.tool, input.mode, "request" if response is None else "response", (monotonic() - started) * 1_000_000))
        except Exception:
            pass
    if response is not None:
        return response
    return original(input)


async def guard_remote_async(original: Callable[[RemoteEnvelope], R], input: RemoteEnvelope, options: RemoteOptions) -> RemoteResponse | R:
    """Run the explicit blocking HTTP API off the caller's event loop."""
    return await asyncio.to_thread(guard_remote, original, input, options)
