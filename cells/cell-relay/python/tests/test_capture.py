from __future__ import annotations

import unittest
from agindow.capture import CAP, capture


class CaptureTests(unittest.TestCase):
    def test_cycles_and_opaque_values_remain_unavailable(self) -> None:
        cyclic: list[object] = []
        cyclic.append(cyclic)
        for value in (cyclic, object(), float("inf"), {1: "non-string key"}):
            with self.subTest(value=type(value).__name__), self.assertRaises(ValueError):
                capture(value)

    def test_oversized_scalar_budget_rejects_before_json_serialization(self) -> None:
        from unittest.mock import patch
        value = ["x" * 1024] * (CAP // 1024 + 1)
        with patch("agindow.capture.json.dumps", side_effect=AssertionError("unbounded serialization")):
            with self.assertRaises(ValueError):
                capture(value)
