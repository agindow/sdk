from __future__ import annotations
import json
import unittest
from agindow import RenameRule, ScalarRule, SelectiveRepair, Verdict, Mode, Operation
from agindow.selective import SelectiveEngine
from agindow.types import EngineCall


class Call:
    request = Verdict()
    def finish(self, value: object) -> Verdict:
        return Verdict()
    def close(self) -> None:
        pass


class Base:
    def begin(self, value: object, mode: Mode, operation: Operation, fail_closed: bool, tool: str) -> EngineCall:
        return Call()


class PreservationTests(unittest.TestCase):
    def test_candidate_cannot_change_values_siblings_unselected_elements_or_whitespace(self) -> None:
        rule = RenameRule('nested', 'request', 'old', 'new', 'r1', 1, ('/items/0',), 'c1', 'owner-review', 4102444800000)
        for body in [
            '{"items":[{"new":2},{"old":2}],"keep":7}',
            '{"items":[{"new":1},{"old":2}],"keep":8}',
            '{"items":[{"new":1},{"new":2}],"keep":7}',
            ' {"items":[{"new":1},{"old":2}],"keep":7}',
        ]:
            def prepare(before: str, rule: RenameRule | ScalarRule, mode: Mode) -> str:
                return json.dumps({'matched': True, 'applied': True, 'halt': False, 'body': body})
            engine = SelectiveEngine(Base(), SelectiveRepair((rule,), lambda _: self.fail('must reject before semantic verification')), prepare)
            with self.assertRaisesRegex(ValueError, 'preservation'):
                engine.begin({'items': [{'old': 1}, {'old': 2}], 'keep': 7}, 'repair', 'read', True, 'nested')
