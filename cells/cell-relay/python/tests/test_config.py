from __future__ import annotations

import os
import tempfile
import unittest
import warnings
from pathlib import Path
from unittest.mock import patch
from agindow import Agindow, AgindowSetupError, Config
from test_sdk import Engine


class ConfigurationTests(unittest.TestCase):
    def test_source_precedence_and_invalid_shadowed_environment(self) -> None:
        previous = Path.cwd()
        with tempfile.TemporaryDirectory(prefix="agindow-config-") as directory, patch.dict(os.environ, {}, clear=True):
            try:
                root = Path(directory)
                (root / ".agindowrc").write_text('{"mode":"repair","buffer":3}')
                child = root / "child"
                child.mkdir()
                os.chdir(child)
                sdk = Agindow(engine=Engine())
                self.assertEqual((sdk.config.mode, sdk.config.buffer), ("repair", 3))
                os.environ["AGINDOW_MODE"] = "strict"
                self.assertEqual(Agindow(engine=Engine()).config.mode, "strict")
                self.assertEqual(Agindow(mode="observe", engine=Engine()).config.mode, "observe")
                os.environ["AGINDOW_MODE"] = "bad"
                with self.assertRaises(AgindowSetupError):
                    Agindow(Config(mode="observe"), engine=Engine())
            finally:
                os.chdir(previous)

    def test_corrupt_file_warns_without_breaking_startup(self) -> None:
        previous = Path.cwd()
        with tempfile.TemporaryDirectory(prefix="agindow-config-") as directory, patch.dict(os.environ, {}, clear=True):
            try:
                root = Path(directory)
                (root / ".agindowrc").write_text("{")
                os.chdir(root)
                with warnings.catch_warnings(record=True) as observed:
                    warnings.simplefilter("always")
                    sdk = Agindow(engine=Engine())
                self.assertEqual(sdk.config.mode, "observe")
                self.assertTrue(any("cannot read valid" in str(item.message) for item in observed))
            finally:
                os.chdir(previous)

    def test_invalid_setup_options_throw_before_any_call(self) -> None:
        sdk = Agindow(engine=Engine())
        variants: tuple[dict[str, object], ...] = ({"mode": "bad"}, {"buffer": -1}, {"fail_closed": "false"}, {"unknown": True})
        for changes in variants:
            with self.subTest(changes=changes), self.assertRaises(AgindowSetupError):
                sdk.configure(**changes)
