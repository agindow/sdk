"""Versioned admission and independent predicates shared with the Node host."""
import unittest
from dataclasses import replace
from agindow import Agindow, Config, RenameRule, SelectiveRepair, AgindowSetupError, AgindowDriftError
from agindow.repair_facts import RepairFacts, RepairConstraint


class RepairFactsTests(unittest.TestCase):
    facts = RepairFacts(version=1, recipe='rename', tool='reviewed', side='request', contract='schema-2',
                        migration='migration-2', provenance='owner-review-7', expires_at=4102444800000,
                        parents=('',), old=RepairConstraint('old', 'string'), new=RepairConstraint('next', 'string'))
    rule = RenameRule(tool='reviewed', side='request', source='old', target='next', migration='migration-2', confidence=1, facts=facts)

    def sdk(self, rule: RenameRule | None = None, mode: str = 'repair') -> Agindow:
        return Agindow(mode=mode, selective_repair=SelectiveRepair((rule or self.rule,), lambda _: True))

    def test_predicates_refuse_even_when_callback_approves(self) -> None:
        sdk = self.sdk()
        self.assertTrue(sdk.ready, str(sdk.load_error))
        self.assertEqual(sdk.guard(lambda x: x, tool='reviewed')({'old': 'Ada', 'id': 1}), {'next': 'Ada', 'id': 1})
        for value in [{'old': 7}, {'old': None}, {'old': {'name': 'Ada'}}]:
            self.assertIs(sdk.guard(lambda x: x, tool='reviewed')(value), value)
        with self.assertRaises(AgindowDriftError):
            self.sdk(mode='strict').guard(lambda x: x, tool='reviewed')({'old': 7})

    def test_invalid_version_scope_revisions_and_constraints(self) -> None:
        for facts in [replace(self.facts, version=2), replace(self.facts, contract=''),
                      replace(self.facts, migration='other'), replace(self.facts, tool='other'),
                      replace(self.facts, side='response'), replace(self.facts, provenance=''),
                      replace(self.facts, parents=('/x',)), replace(self.facts, expires_at=0),
                      replace(self.facts, new=RepairConstraint('next', 'number'))]:
            with self.assertRaises(AgindowSetupError):
                self.sdk(replace(self.rule, facts=facts))

    def test_expiry_scope_and_observe_preserve_identity(self) -> None:
        value = {'old': 'Ada'}
        for sdk in [self.sdk(replace(self.rule, facts=replace(self.facts, expires_at=1))), self.sdk(mode='observe')]:
            self.assertIs(sdk.guard(lambda x: x, tool='reviewed')(value), value)
        self.assertIs(self.sdk().guard(lambda x: x, tool='other')(value), value)

    def test_shared_value_predicate_corpus(self) -> None:
        import json
        from pathlib import Path
        from typing import cast
        from agindow import RepairValueType
        fixtures = json.loads((Path(__file__).parents[2] / 'node/parity/repair-facts.json').read_text())
        for fixture in fixtures:
            kind = cast(RepairValueType, fixture['type'])
            facts = replace(self.facts, old=RepairConstraint('old', kind), new=RepairConstraint('next', kind))
            sdk = self.sdk(replace(self.rule, facts=facts))
            value = {'old': fixture['value'], 'id': 1}
            result = sdk.guard(lambda x: x, tool='reviewed')(value)
            if fixture['accepted']:
                self.assertEqual(result, {'next': fixture['value'], 'id': 1})
            else:
                self.assertIs(result, value)

    def test_verifier_fault_async_and_late_expiry(self) -> None:
        from typing import Callable, cast
        from unittest.mock import patch
        from agindow import RepairCheck
        def fault(_: RepairCheck) -> bool:
            raise RuntimeError('independent verifier unavailable')
        async def asynchronous(_: RepairCheck) -> bool:
            return True
        for verifier in (fault, cast(Callable[[RepairCheck], bool], asynchronous)):
            sdk = Agindow(mode='repair', selective_repair=SelectiveRepair((self.rule,), verifier))
            value = {'old': 'Ada'}
            self.assertIs(sdk.guard(lambda x: x, tool='reviewed')(value), value)
            self.assertEqual(sum(e.type == 'bypass' for e in sdk.recent()), 1)
        facts = replace(self.facts, expires_at=2000)
        def late(_: RepairCheck) -> bool:
            clock.return_value = 3
            return True
        sdk = Agindow(mode='repair', selective_repair=SelectiveRepair((replace(self.rule, facts=facts),), late))
        with patch('time.time', return_value=1) as clock:
            value = {'old': 'Ada'}
            self.assertIs(sdk.guard(lambda x: x, tool='reviewed')(value), value)
