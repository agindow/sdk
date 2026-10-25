"""Lifecycle parity uses the same controlled-clock fixture as Node."""
import json
from pathlib import Path
import unittest
from unittest.mock import patch
from agindow.machine_auth import AccessCredential, ProofSigner, RemoteAuth, canonical_target
from agindow import AgindowSetupError, RemoteEnvelope, RemoteOptions, guard_remote

FIXTURE = json.loads((Path(__file__).parents[2] / 'node/parity/auth.json').read_text())

class MachineAuthTests(unittest.TestCase):
    def signer(self) -> ProofSigner:
        return ProofSigner('AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA', lambda value: b'fixture-signature')

    def test_acquires_only_outside_calls_and_expires_with_one_bypass(self) -> None:
        clock = [float(FIXTURE['now'])]
        acquired: list[str] = []
        def acquire(jkt: str) -> AccessCredential:
            acquired.append(jkt)
            return AccessCredential('fixture-access-secret', FIXTURE['expiresAt'])
        auth = RemoteAuth(acquire, self.signer(), now=lambda: clock[0], schedule=False)
        headers = auth.headers('POST', FIXTURE['target'])
        self.assertIsNotNone(headers)
        self.assertEqual(len(acquired), 1)
        self.assertNotIn('fixture-access-secret', repr(auth))
        self.assertEqual(canonical_target(FIXTURE['target']), FIXTURE['canonical'])
        events: list[object] = []
        clock[0] = FIXTURE['expiredAt']
        with patch('agindow.remote._send') as send:
            result = guard_remote(lambda _: 'fallback', RemoteEnvelope('read','c','read',{},'observe'),
                                  RemoteOptions(FIXTURE['target'], auth=auth, on_event=events.append))
            self.assertEqual(result, 'fallback')
            send.assert_not_called()
        self.assertEqual(len(events), 1)
        self.assertNotIn('fixture-access-secret', repr(events))
        auth.close()

    def test_refresh_failure_preserves_valid_token_without_silent_fallback(self) -> None:
        clock = [float(FIXTURE['now'])]
        fail = [False]
        def acquire(jkt: str) -> AccessCredential:
            if fail[0]:
                raise ValueError('fixture-assertion-secret')
            return AccessCredential('fixture-token', FIXTURE['expiresAt'])
        auth = RemoteAuth(acquire, self.signer(), now=lambda: clock[0], schedule=False)
        fail[0] = True
        clock[0] = FIXTURE['refreshAt']
        self.assertFalse(auth.refresh())
        self.assertIsNotNone(auth.headers('POST', FIXTURE['target']))
        clock[0] = FIXTURE['expiredAt']
        self.assertIsNone(auth.headers('POST', FIXTURE['target']))
        auth.close()

    def test_setup_error_redacts_assertion_and_legacy_requires_explicit_switch(self) -> None:
        def acquire(jkt: str) -> AccessCredential:
            raise ValueError('fixture-assertion-secret')
        with self.assertRaisesRegex(AgindowSetupError, 'keyless identity unavailable'):
            RemoteAuth(acquire, self.signer(), schedule=False)
        diagnostics: list[str] = []
        auth = RemoteAuth(acquire, self.signer(), schedule=False,
                          allow_legacy_fallback=True, legacy_key='agw_test_'+'a'*32,
                          on_diagnostic=diagnostics.append)
        self.assertIn('legacy_unbound', diagnostics)
        self.assertNotIn('fixture-assertion-secret', repr(diagnostics))
        auth.close()

    def test_direct_legacy_key_is_validated_before_transport(self) -> None:
        with self.assertRaisesRegex(AgindowSetupError, 'invalid legacy key'):
            guard_remote(lambda _: 'fallback', RemoteEnvelope('read','c','read',{},'observe'),
                         RemoteOptions(FIXTURE['target'], api_key='unprefixed-secret'))

    def test_proof_bytes_match_shared_rust_node_fixture(self) -> None:
        from base64 import urlsafe_b64decode
        fixture = json.loads((Path(__file__).parents[2] / 'node/parity/auth-proof.json').read_text())
        signing_input, signature = fixture['proof'].rsplit('.', 1)
        def sign(value: bytes) -> bytes:
            self.assertEqual(value, signing_input.encode())
            return urlsafe_b64decode(signature + '==')
        signer = ProofSigner(fixture['x'], sign)
        self.assertEqual(signer.thumbprint, fixture['jkt'])
        with patch('agindow.machine_auth.uuid4', return_value='parity-proof'):
            self.assertEqual(signer.proof(fixture['token'], 'POST', FIXTURE['target'], FIXTURE['now']), fixture['proof'])

    def test_refresh_timer_runs_sixty_seconds_before_expiry(self) -> None:
        from typing import Callable
        clock = [float(FIXTURE['now'])]
        scheduled: list[tuple[float, Callable[[], bool]]] = []
        acquired: list[str] = []
        class FakeTimer:
            daemon = False
            def start(self) -> None:
                pass
            def cancel(self) -> None:
                pass
        def timer(delay: float, callback: Callable[[], bool]) -> FakeTimer:
            scheduled.append((delay, callback))
            return FakeTimer()
        def acquire(jkt: str) -> AccessCredential:
            acquired.append(jkt)
            return AccessCredential('fixture-token', int(clock[0])+300)
        with patch('agindow.machine_auth.Timer', side_effect=timer):
            auth = RemoteAuth(acquire, self.signer(), now=lambda: clock[0])
            self.assertEqual(scheduled[0][0], 240)
            clock[0] = FIXTURE['refreshAt']
            self.assertTrue(scheduled[0][1]())
            self.assertEqual(len(acquired), 2)
            auth.close()
