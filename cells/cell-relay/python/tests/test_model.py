from __future__ import annotations

import unittest
from pydantic import BaseModel
from agindow import Agindow, AgindowDriftError, Config
from test_wasm import selection


class ModelTests(unittest.TestCase):
    def test_decorated_method_inspects_model_without_serializing_receiver(self) -> None:
        sdk = Agindow(Config(mode="repair"), selection=selection("request"))
        calls = 0

        class Input(BaseModel):
            value: int

        class Client:
            @sdk.guard(tool="op", operation="write")
            def invoke(self, value: Input) -> Input:
                nonlocal calls
                calls += 1
                return value

        original = Input(value=100)
        with self.assertRaises(AgindowDriftError):
            Client().invoke(original)
        self.assertEqual(calls, 0)
        self.assertEqual(original.value, 100)
        self.assertFalse(any(event.type == "bypass" for event in sdk.recent()))
