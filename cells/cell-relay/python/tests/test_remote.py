"""TI-41 transport failures retain original Python results and exceptions."""
from __future__ import annotations
import unittest
from unittest.mock import patch
from agindow.remote import RemoteEnvelope, RemoteOptions, guard_remote
from agindow.types import Event

class RemoteTests(unittest.TestCase):
    def test_every_5xx_fails_open_once_for_every_operation(self) -> None:
        for operation in ("read", "write", "unknown"):
            for status in range(500, 600):
                events: list[Event] = []
                value = object()
                calls: list[RemoteEnvelope] = []
                envelope = RemoteEnvelope("op", "c", operation, {}, "repair", "same")
                def original(input: RemoteEnvelope) -> object:
                    calls.append(input)
                    return value
                with patch("agindow.remote._send", return_value=(status, {}, b"internal")):
                    self.assertIs(guard_remote(original, envelope, RemoteOptions("http://localhost/v1/guard", on_event=events.append)), value)
                self.assertEqual(calls, [envelope])
                self.assertEqual([event.type for event in events], ["bypass"])

    def test_refusal_timeout_reset_preserve_original_error(self) -> None:
        for fault in (ConnectionRefusedError(), TimeoutError(), ConnectionResetError()):
            error = RuntimeError("upstream")
            def original(_input: RemoteEnvelope) -> object:
                raise error
            with patch("agindow.remote._send", side_effect=fault):
                with self.assertRaises(RuntimeError) as caught:
                    guard_remote(original, RemoteEnvelope("op", "c", "write", {}, "strict", "same"), RemoteOptions("http://localhost/v1/guard"))
                self.assertIs(caught.exception, error)

    def test_exact_policy_feedback_halts_without_fallback(self) -> None:
        import json
        from agindow.types import AgindowDriftError
        payload = {"error": "drift_halted", "tool": "op", "drift_class": "semantic", "drift_id": "d", "hint": "Review drift", "audit": "/v1/audit/aud_test"}
        with patch("agindow.remote._send", return_value=(409, {}, json.dumps(payload).encode())):
            with self.assertRaises(AgindowDriftError) as caught:
                guard_remote(lambda _: self.fail("halt dispatched"), RemoteEnvelope("op", "c", "write", {}, "strict", "same"), RemoteOptions("http://localhost/v1/guard"))
            self.assertEqual(caught.exception.code, "drift_halted")

    def test_success_body_is_returned_unchanged(self) -> None:
        from agindow.remote import RemoteResponse
        body = b' {"number":1e400}\n'
        with patch("agindow.remote._send", return_value=(201, {}, body)):
            response = guard_remote(lambda _: self.fail("success fallback"), RemoteEnvelope("op", "c", "read", {}, "observe"), RemoteOptions("http://localhost/v1/guard"))
            self.assertIsInstance(response, RemoteResponse)
            if not isinstance(response, RemoteResponse):
                raise AssertionError("remote response expected")
            self.assertEqual(response.status, 201)
            self.assertIs(response.body, body)

    def test_completed_upstream_5xx_is_preserved_without_reexecution(self) -> None:
        from agindow.remote import RemoteResponse
        events: list[Event] = []
        with patch("agindow.remote._send", return_value=(503, {"x-agindow-status": "bypassed"}, b"upstream failure")):
            response = guard_remote(lambda _: self.fail("completed write replayed"), RemoteEnvelope("op", "c", "write", {}, "repair", "same"),
                                    RemoteOptions("http://localhost/v1/guard", on_event=events.append))
            self.assertIsInstance(response, RemoteResponse)
            self.assertEqual([event.type for event in events], ["bypass"])

    def test_null_arguments_remain_an_explicit_envelope_field(self) -> None:
        def send(_endpoint: str, _options: RemoteOptions, payload: bytes | None) -> tuple[int, dict[str, str], bytes]:
            self.assertIsNotNone(payload)
            self.assertIn('"args": null', (payload or b"").decode())
            return 200, {}, b"{}"
        with patch("agindow.remote._send", side_effect=send):
            guard_remote(lambda _: self.fail("unexpected fallback"), RemoteEnvelope("op", "c", "read", None, "observe"), RemoteOptions("http://localhost/v1/guard"))
