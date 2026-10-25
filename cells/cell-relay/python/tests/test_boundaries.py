from __future__ import annotations

import asyncio
import gc
import unittest
import weakref
from unittest.mock import patch
from agindow import Agindow, AgindowDriftError, Config, PolicyFeedback, Verdict, VerifiedRepair
from agindow.types import EngineCall, Mode, Operation
from agindow.wasm import WasmEngine
from test_sdk import Call, Engine, HALT


class Boundaries(unittest.TestCase):
    def test_load_failure_is_loud_once_and_preserves_reference(self) -> None:
        with patch("agindow.wasm.WasmEngine", side_effect=RuntimeError("fixture load failure")):
            sdk = Agindow(mode="strict")
        value = object()
        self.assertFalse(sdk.ready)
        self.assertIsInstance(sdk.load_error, RuntimeError)
        self.assertIs(sdk.guard(lambda: value)(), value)
        self.assertEqual([e.type for e in sdk.recent()], ["bypass"])

    def test_actual_guest_trap_retires_instance_and_never_reenters_it(self) -> None:
        engine = WasmEngine()
        with self.assertRaises(Exception):
            engine._invoke("guestcall_request", 0)
        self.assertTrue(engine.retired)
        sdk = Agindow(Config(mode="strict"), engine=engine)
        value = object()
        for _ in range(2):
            self.assertIs(sdk.guard(lambda: value)(), value)
        self.assertEqual(sum(e.type == "bypass" for e in sdk.recent()), 2)
        engine.close()

    def test_result_decoding_fault_retires_the_consumed_guest(self) -> None:
        from test_wasm import selection
        engine = WasmEngine(selection=selection("response"))
        call = engine.begin({}, "strict", "write", True, "op")
        with patch.object(engine, "_output", side_effect=ValueError("invalid result")):
            with self.assertRaises(ValueError):
                call.finish({"value": 100})
        self.assertTrue(engine.retired)
        engine.close()

    def test_shared_runtime_callbacks_do_not_pin_unclosed_guests(self) -> None:
        engine = WasmEngine()
        reference = weakref.ref(engine)
        del engine
        gc.collect()
        self.assertIsNone(reference())

    def test_cleanup_fault_suppresses_pending_policy_halt(self) -> None:
        engine = Engine(Call(request=HALT, fault="close"))
        sdk = Agindow(Config(mode="strict"), engine=engine)
        value = object()
        self.assertIs(sdk.guard(lambda: value)(), value)
        self.assertEqual(sum(e.type == "bypass" for e in sdk.recent()), 1)

    def test_feedback_fields_and_original_exception_chain_are_retained(self) -> None:
        feedback = PolicyFeedback("drift_blocked", "tool", "semantic", "drift-1", "Review amount units", "https://example.test/a/1")
        sdk = Agindow(Config(mode="repair"), engine=Engine(Call(response=Verdict(halt=True, feedback=feedback))))
        original = ValueError("upstream")

        def upstream() -> None:
            raise original

        with self.assertRaises(AgindowDriftError) as caught:
            sdk.guard(upstream, tool="tool", operation="write")()
        error = caught.exception
        self.assertEqual((error.code, error.tool, error.drift_class, error.drift_id, error.audit_url),
                         ("drift_blocked", "tool", "semantic", "drift-1", "https://example.test/a/1"))
        self.assertIs(error.__cause__, original)
        self.assertIn("do not blindly replay", error.hint)

    def test_unprojectable_request_repair_does_not_claim_application(self) -> None:
        repair = Verdict(drift_class="shape", confidence=.99, findings=1, repair=VerifiedRepair({"bad": True}))
        sdk = Agindow(Config(mode="repair"), engine=Engine(Call(request=repair)))
        original = object()
        self.assertIs(sdk.guard(lambda a, b: a)(original, 2), original)
        self.assertEqual(sum(e.type == "repair" for e in sdk.recent()), 0)
        self.assertEqual(sum(e.type == "bypass" for e in sdk.recent()), 1)

    def test_verified_repairs_keep_observe_immutable_and_repair_events_detached(self) -> None:
        original = {"old": 1}
        replacement = {"new": 1}
        repair = Verdict(drift_class="shape", confidence=.99, findings=1, repair=VerifiedRepair(replacement))
        sdk = Agindow(Config(mode="observe"), engine=Engine(Call(response=repair)))
        fn = sdk.guard(lambda: original)
        self.assertIs(fn(), original)
        sdk.configure(mode="repair")
        self.assertIs(fn(), replacement)
        self.assertEqual(original, {"old": 1})
        events = [e for e in sdk.recent() if e.type == "repair"]
        self.assertEqual(len(events), 1)
        self.assertFalse(events[0].byte_identical)
        self.assertIsNotNone(events[0].verdict)
        if events[0].verdict:
            self.assertIsNone(events[0].verdict.repair)


class AsyncBoundaries(unittest.IsolatedAsyncioTestCase):
    async def test_cancellation_disposes_the_suspended_engine_call(self) -> None:
        call = Call()
        sdk = Agindow(engine=Engine(call))
        started = asyncio.Event()

        @sdk.guard
        async def upstream() -> None:
            started.set()
            await asyncio.Event().wait()

        task = asyncio.create_task(upstream())
        await started.wait()
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task
        self.assertEqual(call.closed, 1)

    async def test_concurrent_scopes_and_event_sinks_use_frozen_configuration(self) -> None:
        engine = Engine(Call(response=HALT))
        first: list[str] = []
        second: list[str] = []
        sdk = Agindow(Config(mode="observe", on_event=lambda e: first.append(e.type)), engine=engine)
        started = asyncio.Event()
        release = asyncio.Event()

        @sdk.guard
        async def upstream() -> int:
            started.set()
            await release.wait()
            return 42

        pending = asyncio.create_task(upstream())
        await started.wait()
        sdk.configure(on_event=lambda e: second.append(e.type))
        release.set()
        self.assertEqual(await pending, 42)
        self.assertEqual(first, ["drift"])
        self.assertEqual(second, [])

        async def scoped(mode: Mode) -> bool:
            async with sdk.context(mode=mode):
                await asyncio.sleep(0)
                try:
                    await upstream()
                    return True
                except AgindowDriftError:
                    return False
        self.assertEqual(await asyncio.gather(scoped("observe"), scoped("strict")), [True, False])
