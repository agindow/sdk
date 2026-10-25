from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

PROJECT = Path(__file__).resolve().parents[1]


class ParityMutations(unittest.TestCase):
    def test_actual_python_only_mutations_fail_cross_language_comparison(self) -> None:
        original = (PROJECT / "src/agindow/primitive.py").read_text()
        mutations = {
            "policy_halt_disabled": ('halted = verdict.halt and self.config.mode != "observe"', 'halted = False'),
            "availability_throws": ('self.bypass("request", True)\n            return value',
                                    'self.bypass("request", True)\n            raise RuntimeError("mutated bypass")'),
        }
        runner = PROJECT.parent / "node/parity/run.mjs"
        for name, (before, after) in mutations.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory(prefix="agindow-python-mutation-") as directory:
                self.assertEqual(original.count(before), 1)
                root = Path(directory)
                shutil.copytree(PROJECT / "src/agindow", root / "agindow",
                                ignore=shutil.ignore_patterns("__pycache__"))
                (root / "agindow/primitive.py").write_text(original.replace(before, after, 1))
                environment = {**os.environ, "PYTHONPATH": str(root)}
                result = subprocess.run(["node", str(runner)], cwd=root, env=environment,
                                        capture_output=True, text=True, timeout=30)
                self.assertNotEqual(result.returncode, 0, "one-sided wrapper mutation escaped parity")
                self.assertTrue("divergence" in result.stderr or "mutated bypass" in result.stderr, result.stderr)
