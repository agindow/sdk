from dataclasses import replace
import asyncio
import unittest
from agindow import Agindow, Config, ScalarFacts, ScalarRule, SelectiveRepair, AgindowDriftError


class ScalarRepairTests(unittest.TestCase):
    def facts(self) -> ScalarFacts:
        return ScalarFacts(version=1, recipe='string-map', tool='pay', side='request', contract='v2', migration='review-1',
                           provenance='owner', expires_at=4102444800000, pointer='/value', mapping=(('old', 'new'),))

    def sdk(self, facts: ScalarFacts | None = None, mode: str = 'repair') -> Agindow:
        from typing import cast
        from agindow import Mode
        facts = facts or self.facts()
        sdk = Agindow(Config(mode=cast(Mode, mode)), selective_repair=SelectiveRepair((ScalarRule(facts.tool, facts.side, facts.migration, 1, facts),), lambda _: True))
        self.assertTrue(sdk.ready, str(sdk.load_error))
        return sdk

    def test_mapping_null_and_exact_integer_scale(self) -> None:
        sdk = self.sdk()
        self.assertEqual(sdk.guard(lambda x: x, tool='pay')({'value': 'old', 'id': 9007199254740993}), {'value': 'new', 'id': 9007199254740993})
        null = self.sdk(replace(self.facts(), recipe='empty-null', mapping=None, null_meaning='absent'))
        self.assertEqual(null.guard(lambda x: x, tool='pay')({'value': ''}), {'value': None})
        scale = self.sdk(replace(self.facts(), recipe='scale', mapping=None, source_unit='seconds', target_unit='milliseconds', numerator='1000', denominator='1',
                                 input_min='-999999999999999999', input_max='999999999999999999', output_min='-999999999999999999', output_max='999999999999999999'))
        self.assertEqual(scale.guard(lambda x: x, tool='pay')({'value': 123}), {'value': 123000})

    def test_refused_semantics_block_writes_but_reads_and_observe_keep_identity(self) -> None:
        value = {'value': 'unknown'}
        sdk = self.sdk()
        with self.assertRaises(AgindowDriftError):
            sdk.guard(lambda x: x, tool='pay', operation='write')(value)
        self.assertIs(sdk.guard(lambda x: x, tool='pay', operation='read')(value), value)
        self.assertIs(self.sdk(mode='observe').guard(lambda x: x, tool='pay', operation='write')(value), value)
        self.assertIs(sdk.guard(lambda x: x, tool='other', operation='write')(value), value)

    def test_async_response_and_original_exception_once(self) -> None:
        sdk = self.sdk(replace(self.facts(), side='response'))
        @sdk.guard(tool='pay')
        async def read() -> dict[str, str]:
            return {'value': 'old'}
        self.assertEqual(asyncio.run(read()), {'value': 'new'})
        calls = 0
        error = ValueError('upstream')
        def write(value: object) -> object:
            nonlocal calls
            calls += 1
            raise error
        with self.assertRaises(ValueError) as result:
            self.sdk().guard(write, tool='pay', operation='write')({'value': 'old'})
        self.assertIs(result.exception, error)
        self.assertEqual(calls, 1)

    def test_shared_scalar_corpus_and_independent_arithmetic(self) -> None:
        import json
        from pathlib import Path
        from typing import TypedDict, cast
        from agindow.wasm import WasmEngine
        from agindow.scalar import fixed
        class Fixture(TypedDict):
            id: str
            before: str
            after: str | None
            facts: dict[str, object]
            confidence: float
        fixtures = cast(list[Fixture], json.loads((Path(__file__).parents[2] / 'node/parity/scalar-repairs.json').read_text()))
        guest = WasmEngine()
        for row in fixtures:
            try:
                result = json.loads(guest.prepare_scalar(row['before'], json.dumps(row['facts']), float(row['confidence']), 'repair'))
                if row['after'] is not None:
                    self.assertEqual(result['body'], row['after'], row['id'])
                    if row['facts']['recipe'] == 'scale':
                        from agindow.scalar_tokens import scalar_token
                        pointer = str(row['facts']['pointer'])
                        start, end, _ = scalar_token(row['before'], pointer)
                        target_start, target_end, _ = scalar_token(row['after'], pointer)
                        numerator, denominator = int(str(row['facts']['numerator'])), int(str(row['facts']['denominator']))
                        self.assertEqual(fixed(row['before'][start:end]) * numerator,
                                         fixed(row['after'][target_start:target_end]) * denominator)
                else:
                    self.assertFalse(result['applied'], row['id'])
            except (RuntimeError, ValueError):
                self.assertIsNone(row['after'], row['id'])

    def test_per_source_equivalence_and_projection_precision_refusal(self) -> None:
        from agindow import AgindowSetupError
        facts = replace(self.facts(), mapping=(('a', 'same'), ('b', 'same')), equivalence=(('a', 'review-a'),))
        with self.assertRaises(AgindowSetupError):
            self.sdk(facts)
        reviewed = replace(facts, equivalence=(('a', 'review-a'), ('b', 'review-b')))
        self.assertEqual(self.sdk(reviewed).guard(lambda x: x, tool='pay')({'value': 'b'}), {'value': 'same'})
        scale = replace(self.facts(), recipe='scale', mapping=None, source_unit='cents', target_unit='dollars', numerator='1', denominator='100',
                        input_min='-999999999999999999', input_max='999999999999999999', output_min='-999999999999999999', output_max='999999999999999999')
        with self.assertRaises(AgindowDriftError) as result:
            self.sdk(scale).guard(lambda x: x, tool='pay', operation='write')({'value': 9007199254740993})
        self.assertEqual(result.exception.verdict.repair_refusal, 'projection-precision')

    def test_verifier_fault_is_one_bypass_and_refusal_blocks(self) -> None:
        from typing import Callable
        from agindow import RepairCheck
        facts = self.facts()
        def fault(_: RepairCheck) -> bool:
            raise RuntimeError('verifier unavailable')
        verifiers: tuple[Callable[[RepairCheck], bool], ...] = (lambda _: False, fault)
        for verify in verifiers:
            sdk = Agindow(Config(mode='repair'), selective_repair=SelectiveRepair((ScalarRule(facts.tool, facts.side, facts.migration, 1, facts),), verify))
            calls: list[object] = []
            def dispatch(value: object) -> object:
                calls.append(value)
                return value
            value = {'value': 'old'}
            if verify is fault:
                self.assertIs(sdk.guard(dispatch, tool='pay', operation='write')(value), value)
                self.assertEqual(len(calls), 1)
                self.assertEqual(sum(e.type == 'bypass' for e in sdk.recent()), 1)
            else:
                with self.assertRaises(AgindowDriftError):
                    sdk.guard(dispatch, tool='pay', operation='write')(value)
                self.assertEqual(calls, [])
