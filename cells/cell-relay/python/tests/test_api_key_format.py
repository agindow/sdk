"""Registry-issued key forms share the unchanged TI-70 fallback parser."""
from __future__ import annotations
import json
from pathlib import Path
import unittest
from agindow.machine_auth import valid_legacy_key

class ApiKeyFormatTests(unittest.TestCase):
    def test_registry_presented_keys_are_compatible(self) -> None:
        source = json.loads((Path(__file__).parents[2] / 'node/parity/api-keys.json').read_text())
        for row in source['keys']:
            with self.subTest(environment=row['environment']):
                self.assertTrue(valid_legacy_key(row['key']))
                self.assertTrue(row['key'].startswith(f"agw_{row['environment']}_{row['public_id']}_"))
                self.assertEqual(row['key'][-4:], row['last4'])
                self.assertLessEqual(len(f"Bearer {row['key']}".encode()), 1024)
