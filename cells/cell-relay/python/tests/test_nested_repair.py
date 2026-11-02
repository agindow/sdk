from dataclasses import replace
import unittest
from agindow import Agindow, Config, RenameRule, SelectiveRepair, AgindowSetupError, Mode


class NestedRepairTests(unittest.TestCase):
    def rule(self) -> RenameRule:
        return RenameRule(tool='nested', side='request', source='old.key', target='new/key', migration='review-1',
                          confidence=1.0, parents=('/items/0', '/items/2'), contract='schema-1',
                          provenance='reviewed-by-owner', expires_at=4102444800000)

    def test_normal_nested_atomic_request(self) -> None:
        sdk = Agindow(Config(mode='repair'), selective_repair=SelectiveRepair((self.rule(),), lambda _: True))
        self.assertTrue(sdk.ready, str(sdk.load_error))
        value = {'items': [{'old.key': 9007199254740993}, {'old.key': 2}, {'old.key': 3}], 'sibling': 4}
        calls: list[object] = []
        def dispatch(value: object) -> object:
            calls.append(value)
            return value
        self.assertEqual(sdk.guard(dispatch, tool='nested')(value),
                         {'items': [{'new/key': 9007199254740993}, {'old.key': 2}, {'new/key': 3}], 'sibling': 4})
        self.assertEqual(len(calls), 1)
        collision = {'items': [{'old.key': 1}, {}, {'old.key': 3, 'new/key': 4}]}
        self.assertIs(sdk.guard(dispatch, tool='nested')(collision), collision)

    def test_expired_and_observe_preserve_identity(self) -> None:
        value = {'items': [{'old.key': 1}, {}, {'old.key': 3}]}
        configurations: list[tuple[Mode, RenameRule]] = [('observe', self.rule()), ('repair', replace(self.rule(), expires_at=1))]
        for mode, rule in configurations:
            sdk = Agindow(Config(mode=mode), selective_repair=SelectiveRepair((rule,), lambda _: True))
            self.assertIs(sdk.guard(lambda x: x, tool='nested')(value), value)

    def test_invalid_selections_fail_at_setup(self) -> None:
        for parents in [('/x',) * 17, ('/x', '/x'), ('', '/old.key'), ('/bad~2',)]:
            with self.assertRaises(AgindowSetupError):
                Agindow(selective_repair=SelectiveRepair((replace(self.rule(), parents=parents),), lambda _: True))

    def test_shared_rust_node_python_corpus(self) -> None:
        import json
        from pathlib import Path
        from typing import TypedDict, cast
        from agindow.rename_preservation import rename_plan
        from agindow.wasm import WasmEngine
        class Fixture(TypedDict):
            id: str
            source: str
            target: str
            parents: list[str]
            before: str
            after: str | None
        fixtures = cast(list[Fixture], json.loads((Path(__file__).parents[2] / 'node/parity/nested-renames.json').read_text()))
        guest = WasmEngine()
        for fixture in fixtures:
            rule = replace(self.rule(), source=fixture['source'], target=fixture['target'], parents=tuple(fixture['parents']))
            try:
                result = json.loads(guest.prepare_rename(fixture['before'], rule, 'repair'))
                self.assertEqual(result['applied'], fixture['after'] is not None, fixture['id'])
                if fixture['after'] is not None:
                    self.assertEqual(result['body'], fixture['after'], fixture['id'])
                    expected, _ = rename_plan(fixture['before'], rule.source, rule.target, rule.parents or ('',))
                    self.assertEqual(expected, fixture['after'], fixture['id'])
            except (RuntimeError, ValueError):
                self.assertIsNone(fixture['after'], fixture['id'])

    def test_async_response_and_exception_identity(self) -> None:
        import asyncio
        sdk = Agindow(Config(mode='strict'), selective_repair=SelectiveRepair((replace(self.rule(), side='response'),), lambda _: True))
        @sdk.guard(tool='nested')
        async def read() -> dict[str, object]:
            return {'items': [{'old.key': 1}, {}, {'old.key': 3}]}
        self.assertEqual(asyncio.run(read()), {'items': [{'new/key': 1}, {}, {'new/key': 3}]})
        request = Agindow(Config(mode='repair'), selective_repair=SelectiveRepair((self.rule(),), lambda _: True))
        error = RuntimeError('upstream')
        calls = 0
        def dispatch(value: object) -> object:
            nonlocal calls
            calls += 1
            raise error
        with self.assertRaises(RuntimeError) as caught:
            request.guard(dispatch, tool='nested')({'items': [{'old.key': 1}, {}, {'old.key': 3}]})
        self.assertIs(caught.exception, error)
        self.assertEqual(calls, 1)

    def test_work_and_capture_exhaustion_dispatch_original_once(self) -> None:
        for work, value in [(1, {'items': [{'old.key': 1}, {}, {'old.key': 3}]}),
                            (200_000_000, {'pad': 'x' * 1_048_576})]:
            sdk = Agindow(Config(mode='repair'), work=work, selective_repair=SelectiveRepair((self.rule(),), lambda _: True))
            calls: list[object] = []
            def dispatch(value: object) -> object:
                calls.append(value)
                return value
            self.assertIs(sdk.guard(dispatch, tool='nested')(value), value)
            self.assertEqual(len(calls), 1)
            self.assertEqual(sum(event.type == 'bypass' for event in sdk.recent()), 1)

    def test_unpaired_surrogate_keys_and_pointers_are_named_setup_errors(self) -> None:
        for rule in [replace(self.rule(), source='\ud800'), replace(self.rule(), parents=('/\ud800',))]:
            with self.assertRaises(AgindowSetupError):
                Agindow(selective_repair=SelectiveRepair((rule,), lambda _: True))

    def test_versioned_facts_constrain_the_entire_concrete_nested_batch(self) -> None:
        from agindow import RepairFacts, RepairConstraint
        rule = self.rule()
        facts = RepairFacts(1, 'rename', rule.tool, rule.side, 'schema-1', rule.migration,
                            'reviewed-by-owner', 4102444800000, rule.parents or (),
                            RepairConstraint(rule.source, 'number'), RepairConstraint(rule.target, 'number'))
        sdk = Agindow(Config(mode='repair'), selective_repair=SelectiveRepair((replace(rule, facts=facts),), lambda _: True))
        value = {'items': [{'old.key': 1}, {}, {'old.key': 3}]}
        self.assertEqual(sdk.guard(lambda x: x, tool='nested')(value), {'items': [{'new/key': 1}, {}, {'new/key': 3}]})
        mismatch = {'items': [{'old.key': 1}, {}, {'old.key': 'wrong type'}]}
        self.assertIs(sdk.guard(lambda x: x, tool='nested')(mismatch), mismatch)
