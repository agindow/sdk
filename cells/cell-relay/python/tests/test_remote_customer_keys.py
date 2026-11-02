"""Shared customer keys select the hosted guard's sdk.remote admission scope."""
import json
from pathlib import Path
import unittest
from unittest.mock import MagicMock, patch
from agindow.remote import RemoteOptions, _send

class CustomerRemoteTests(unittest.TestCase):
    def test_shared_customer_key_headers(self) -> None:
        fixtures = json.loads((Path(__file__).resolve().parents[2] / "node/parity/api-keys.json").read_text())
        for row in fixtures["keys"]:
            connection = MagicMock()
            connection.getresponse.return_value.status = 200
            connection.getresponse.return_value.getheaders.return_value = []
            with patch("agindow.remote.HTTPSConnection", return_value=connection):
                _send("https://guard.invalid/v1/guard", RemoteOptions("https://guard.invalid/v1/guard", api_key=row["key"]), b"{}")
            headers = connection.request.call_args.args[3]
            self.assertEqual(headers["authorization"], "Bearer " + row["key"])
            self.assertEqual(headers["x-agindow-key-use"], "sdk.remote")
