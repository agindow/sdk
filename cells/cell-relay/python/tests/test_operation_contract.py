from dataclasses import replace
import json
import unittest
from agindow import OperationScope, OperationSource, compare_operation


class OperationContractTests(unittest.TestCase):
    def test_direction_provenance_and_scope_isolation(self) -> None:
        scope = OperationScope('/charges', 'POST', 'request', 'v1')
        old = OperationSource(scope, 'declared', 'review-a', json.dumps({'type': 'object', 'properties': {'x': {'type': 'string'}}}), True)
        new = OperationSource(scope, 'declared', 'review-b', json.dumps({'type': 'object', 'properties': {'x': {'type': ['string', 'null']}}}), True)
        result = compare_operation(old, new)
        self.assertEqual(result['coverage'], 'complete')
        self.assertIn('compatible', str(result['findings']))
        response = replace(scope, side='response')
        self.assertIn('breaking', str(compare_operation(replace(old, scope=response), replace(new, scope=response))['findings']))
        self.assertEqual(compare_operation(old, replace(new, scope=replace(scope, route='/other')))['findings'], [])

    def test_observed_absence_and_coverage_gaps(self) -> None:
        scope = OperationScope('/charges', 'GET', 'response', 'v1')
        old = OperationSource(scope, 'observed', 'capture-a', '{"nested":{"x":1}}', True)
        new = replace(old, body='{"nested":{}}')
        result = compare_operation(old, new)
        self.assertEqual(result['requiredness'], 'unknown')
        self.assertIn('unknown', str(result['findings']))
        for body in ['{"items":[]}', '{"items":[1,"x"]}', '{"x":1,"x":2}']:
            self.assertEqual(compare_operation(old, replace(new, body=body))['findings'], [])
        self.assertEqual(compare_operation(old, replace(new, complete=False))['findings'], [])
        self.assertEqual(compare_operation(old, new, work=1)['findings'], [])

    def test_shared_operation_corpus(self) -> None:
        from pathlib import Path
        from typing import TypedDict, cast
        from agindow.wasm import WasmEngine
        class Fixture(TypedDict):
            id: str
            input: dict[str, object]
            expected: dict[str, object]
        rows = cast(list[Fixture], json.loads((Path(__file__).parents[2] / 'node/parity/operation-contracts.json').read_text()))
        guest = WasmEngine()
        for row in rows:
            result = json.loads(guest.compare_operation(json.dumps(row['input'])))
            for field in ('coverage', 'requiredness'):
                self.assertEqual(result[field], row['expected'][field], row['id'])
            findings = [{'path': f['path'], 'compatibility': f['compatibility']} for f in result['findings']]
            expected = cast(list[dict[str, str]], row['expected']['findings'])
            self.assertEqual(sorted(findings, key=lambda f: f['path']), sorted(expected, key=lambda f: f['path']), row['id'])
