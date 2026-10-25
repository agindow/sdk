from __future__ import annotations

import unittest
from agindow import wrap, configure


class PublicTests(unittest.TestCase):
    def test_module_wrap_is_typed_callable_entry_point(self) -> None:
        value = object()
        configure(mode="observe")

        def original(item: object) -> object:
            return item

        wrapped = wrap(original)
        self.assertIs(wrapped(value), value)
        self.assertEqual(wrapped.__name__, original.__name__)
