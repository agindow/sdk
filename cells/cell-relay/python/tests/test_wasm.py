from __future__ import annotations

import hashlib
import unittest
from pathlib import Path
from agindow import Agindow, Config, AgindowDriftError


def selection(side: str) -> dict[str, object]:
    return {"tenant": "t", "connection": "c", "vendor": "v", "operation": "op", "segment": "s",
            "fields": [{"side": side, "path": "/value", "baseline": ['{"value":1}'] * 128,
                        "live": ['{"value":100}'] * 128, "enumeration": True}]}


class WasmTests(unittest.TestCase):
    def test_default_is_actual_shared_guest_not_python_null_engine(self) -> None:
        sdk = Agindow()
        self.assertTrue(sdk.ready, str(sdk.load_error))
        value = object()
        self.assertIs(sdk.guard(lambda: value)(), value)
        self.assertEqual(sdk.recent(), ())
        import agindow
        guest = Path(agindow.__file__).parent / "_guest.wasm"
        node = Path(__file__).resolve().parents[2] / "node/dist/guest/agw_node_guest_bg.wasm"
        self.assertEqual(hashlib.sha256(guest.read_bytes()).digest(), hashlib.sha256(node.read_bytes()).digest())
        sdk.close()

    def test_real_statistical_request_halts_before_dispatch(self) -> None:
        sdk = Agindow(Config(mode="repair"), selection=selection("request"))
        self.assertTrue(sdk.ready, str(sdk.load_error))
        calls = 0

        def upstream(value: dict[str, int]) -> dict[str, int]:
            nonlocal calls
            calls += 1
            return value

        fn = sdk.guard(upstream, tool="op", operation="write")
        with self.assertRaises(AgindowDriftError) as caught:
            fn({"value": 100})
        self.assertEqual(calls, 0)
        self.assertEqual(caught.exception.verdict.detectors, 5)
        sdk.configure(mode="observe")
        original = {"value": 100}
        self.assertIs(fn(original), original)
        self.assertEqual(calls, 1)
        self.assertFalse(any(event.type == "bypass" for event in sdk.recent()))

    def test_real_statistical_response_retains_completed_outcome(self) -> None:
        sdk = Agindow(Config(mode="strict"), selection=selection("response"))
        self.assertTrue(sdk.ready, str(sdk.load_error))
        value = {"value": 100}
        with self.assertRaises(AgindowDriftError) as caught:
            sdk.guard(lambda: value, tool="op", operation="write")()
        self.assertIs(caught.exception.outcome, value)
        self.assertEqual(caught.exception.code, "drift_halted")
        self.assertIn("do not blindly replay", caught.exception.hint)

    def test_real_budget_exhaustion_and_scope_mismatch_bypass_once(self) -> None:
        for work, tool in [(0, "op"), (200_000_000, "other")]:
            sdk = Agindow(Config(mode="strict"), selection=selection("request"), work=work)
            self.assertTrue(sdk.ready, str(sdk.load_error))
            original = {"value": 100}
            self.assertIs(sdk.guard(lambda: original, tool=tool, operation="write")(), original)
            self.assertEqual([event.type for event in sdk.recent()].count("bypass"), 1)
