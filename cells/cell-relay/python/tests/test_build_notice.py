"""Package staging must not replace public notices with private repository scope."""
from __future__ import annotations

import hashlib
import tempfile
import unittest
import shutil
import subprocess
import sys
from pathlib import Path


class BuildNoticeTests(unittest.TestCase):
    def test_repeated_staging_uses_public_runtime_notice_and_identical_wasm(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            node = root / "cells/cell-relay/node"
            project = node.parent / "python"
            (node / "dist/guest").mkdir(parents=True)
            (project / "src/agindow").mkdir(parents=True)
            wasm = b"bounded shared guest fixture"
            (node / "dist/guest/agw_node_guest_bg.wasm").write_bytes(wasm)
            (root / "LICENSE").write_text("Apache license fixture")
            (root / "NOTICE").write_text("Private repository scope fixture")
            notice = node.parent / "NOTICE"
            notice.write_text("Public runtime scope fixture")
            (project / "scripts").mkdir()
            script = project / "scripts/build.py"
            shutil.copyfile(Path(__file__).resolve().parents[1] / "scripts/build.py", script)
            def stage() -> str:
                result = subprocess.run([sys.executable, str(script), "--stage-only"],
                                        check=True, capture_output=True, text=True)
                return result.stdout.strip()
            for _ in range(2):
                (project / "NOTICE").write_text("Stale package notice")
                self.assertEqual(stage(), f"shared WASM sha256: {hashlib.sha256(wasm).hexdigest()}")
                self.assertEqual((project / "NOTICE").read_text(), "Public runtime scope fixture")
                self.assertEqual((project / "LICENSE").read_text(), "Apache license fixture")
                self.assertEqual((root / "NOTICE").read_text(), "Private repository scope fixture")
                self.assertEqual((project / "src/agindow/_guest.wasm").read_bytes(), wasm)
            notice.unlink()
            with self.assertRaises(subprocess.CalledProcessError):
                stage()
            self.assertEqual((project / "NOTICE").read_text(), "Public runtime scope fixture")
