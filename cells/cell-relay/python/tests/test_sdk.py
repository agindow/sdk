from __future__ import annotations

import asyncio
import inspect
import unittest
from pydantic import BaseModel
from agindow import Agindow, AgindowDriftError, Config, Verdict, guard
from agindow.types import EngineCall, Mode, Operation

PASS = Verdict()
HALT = Verdict(halt=True, drift_class="semantic", findings=1)


class Call:
    def __init__(self, request: Verdict = PASS, response: Verdict = PASS, fault: str = "") -> None:
        self.request = request
        self.response = response
        self.fault = fault
        self.closed = 0

    def finish(self, value: object) -> Verdict:
        if self.fault == "finish":
            raise RuntimeError("fixture finish fault")
        return self.response

    def close(self) -> None:
        self.closed += 1
        if self.fault == "close":
            raise RuntimeError("fixture close fault")


class Engine:
    def __init__(self, call: Call | None = None, fault: bool = False) -> None:
        self.call = call or Call()
        self.fault = fault
        self.calls = 0

    def begin(self, value: object, mode: Mode, operation: Operation, fail_closed: bool, tool: str) -> EngineCall:
        self.calls += 1
        if self.fault:
            raise RuntimeError("fixture begin fault")
        return self.call


class SDKTests(unittest.TestCase):
    def test_decorator_preserves_model_signature_receiver_and_arguments(self) -> None:
        class Model(BaseModel):
            value: int

        sdk = Agindow(engine=Engine())
        model = Model(value=3)

        class Client:
            @sdk.guard
            def invoke(self, item: Model, *, label: str) -> Model:
                """Fixture callable."""
                self.label = label
                return item

        client = Client()
        self.assertIs(client.invoke(model, label="ok"), model)
        self.assertEqual(client.label, "ok")
        self.assertEqual(client.invoke.__name__, "invoke")
        self.assertEqual(client.invoke.__doc__, "Fixture callable.")
        self.assertIn("label", inspect.signature(client.invoke).parameters)

    def test_internal_faults_bypass_once_and_keep_original_exception(self) -> None:
        error = ValueError("upstream")
        for engine in [Engine(fault=True), Engine(Call(fault="finish")), Engine(Call(fault="close"))]:
            sdk = Agindow(Config(mode="strict"), engine=engine)

            def upstream() -> None:
                raise error

            with self.assertRaises(ValueError) as caught:
                sdk.guard(upstream)()
            self.assertIs(caught.exception, error)
            self.assertEqual([e.type for e in sdk.recent()].count("bypass"), 1)
            self.assertEqual([e.type for e in sdk.recent()].count("error"), 1)

    def test_request_halt_dispatches_zero_times(self) -> None:
        sdk = Agindow(Config(mode="repair"), engine=Engine(Call(request=HALT)))
        calls = 0

        def upstream() -> object:
            nonlocal calls
            calls += 1
            return object()

        with self.assertRaises(AgindowDriftError) as caught:
            sdk.guard(upstream, tool="test", operation="write")()
        self.assertEqual(calls, 0)
        self.assertEqual(caught.exception.code, "drift_blocked")
        self.assertEqual(caught.exception.phase, "request")

    def test_response_halt_retains_outcome_and_chains_upstream_error(self) -> None:
        sdk = Agindow(Config(mode="strict"), engine=Engine(Call(response=HALT)))
        value = object()
        with self.assertRaises(AgindowDriftError) as caught:
            sdk.guard(lambda: value, operation="write")()
        self.assertIs(caught.exception.outcome, value)
        self.assertIn("do not blindly replay", caught.exception.hint)
        original = ValueError("upstream")

        def failed() -> None:
            raise original

        with self.assertRaises(AgindowDriftError) as caught:
            sdk.guard(failed, operation="write")()
        self.assertIs(caught.exception.__cause__, original)

    def test_context_mode_is_scoped_and_observe_never_blocks(self) -> None:
        sdk = Agindow(Config(mode="observe"), engine=Engine(Call(request=HALT)))
        value = object()
        fn = sdk.guard(lambda: value)
        self.assertIs(fn(), value)
        with sdk.context(mode="strict"):
            with self.assertRaises(AgindowDriftError):
                fn()
        self.assertIs(fn(), value)
        self.assertEqual(sdk.config.mode, "observe")

    def test_module_decorator_is_sync_and_reference_preserving(self) -> None:
        value = object()

        @guard(mode="observe")
        def fn() -> object:
            return value

        self.assertIs(fn(), value)
        self.assertFalse(inspect.iscoroutinefunction(fn))

    def test_ring_and_listener_fault_are_contained(self) -> None:
        def listener(event: object) -> None:
            raise RuntimeError("listener")

        sdk = Agindow(Config(buffer=2, on_event=listener), engine=Engine(fault=True))
        fn = sdk.wrap(lambda: 42)
        self.assertEqual(fn(), 42)
        self.assertEqual(len(sdk.recent()), 2)
        sdk.configure(buffer=0)
        self.assertEqual(fn(), 42)
        self.assertEqual(sdk.recent(), ())


class AsyncTests(unittest.IsolatedAsyncioTestCase):
    async def test_async_snapshot_survives_reload_and_task_local_contexts(self) -> None:
        sdk = Agindow(Config(mode="observe"), engine=Engine(Call(response=HALT)))
        started = asyncio.Event()
        release = asyncio.Event()
        value = object()

        @sdk.guard
        async def fn() -> object:
            started.set()
            await release.wait()
            return value

        pending = asyncio.create_task(fn())
        await started.wait()
        sdk.configure(mode="strict")
        release.set()
        self.assertIs(await pending, value)
        with self.assertRaises(AgindowDriftError):
            await fn()
        async with sdk.context(mode="observe"):
            self.assertIs(await fn(), value)
        self.assertEqual(sdk.config.mode, "strict")
