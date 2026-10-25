from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import shutil
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from importlib.metadata import distribution

PROJECT = Path(__file__).resolve().parents[1]


def install_locked_runtime(python: Path) -> None:
    # Exercise our isolated wheel with the already verified locked dependency.
    # Hard links also avoid this host's dyld signature revalidation stall on
    # every new copy of Wasmtime's native library; no SDK source is linked.
    result = subprocess.run([str(python), "-I", "-c", "import site;print(site.getsitepackages()[0])"],
                            check=True, capture_output=True, text=True, timeout=30)
    target = Path(result.stdout.strip())
    files = distribution("wasmtime").files
    if files is None:
        raise RuntimeError("runtime distribution has no file manifest")
    for file in files:
        if ".." in file.parts or file.is_absolute():
            raise RuntimeError("unexpected dependency distribution path")
        destination = target / file
        destination.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.link(file.locate(), destination)
        except OSError:
            shutil.copyfile(file.locate(), destination)


class PackageTests(unittest.TestCase):
    def test_wheel_and_sdist_include_shared_wasm_and_run_outside_checkout(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agindow-python-package-") as directory:
            root = Path(directory)
            output = root / "dist"
            # No build isolation: the isolated env would have to resolve
            # `hatchling==1.32.4` from uv's simple-index cache, which CI cache
            # pruning strips even when the wheel itself stays cached (a frozen
            # sync never needs that index, so it cannot be relied on here).
            # The locked dev environment already provides hatchling, and this
            # test only ever runs under `uv run --frozen` / a frozen sync.
            # Surface stderr on failure so CI shows the cause, not just exit 2.
            build = subprocess.run(["uv", "build", "--offline", "--no-sources", "--no-build-isolation",
                                    "--out-dir", str(output)],
                                   cwd=PROJECT, check=False, capture_output=True, text=True, timeout=60)
            self.assertEqual(build.returncode, 0, build.stderr)
            wheel, = output.glob("*.whl")
            source, = output.glob("*.tar.gz")
            with zipfile.ZipFile(wheel) as archive:
                self.assertIn("agindow/_guest.wasm", archive.namelist())
                self.assertIn("agindow/py.typed", archive.namelist())
                wasm = archive.read("agindow/_guest.wasm")
                self.assertLess(wheel.stat().st_size, 5 * 1024 * 1024)
                self.assertTrue(any(name.endswith("licenses/LICENSE") for name in archive.namelist()))
            with tarfile.open(source) as archive:
                self.assertTrue(any(name.endswith("src/agindow/_guest.wasm") for name in archive.getnames()))
            node = PROJECT.parent / "node/dist/guest/agw_node_guest_bg.wasm"
            self.assertEqual(hashlib.sha256(wasm).digest(), hashlib.sha256(node.read_bytes()).digest())
            environment = {key: value for key, value in os.environ.items() if key not in ("PYTHONPATH", "VIRTUAL_ENV")}
            isolated = root / "venv"
            subprocess.run(["uv", "venv", "--python", sys.executable, str(isolated)],
                           check=True, capture_output=True, timeout=30, env=environment)
            python = isolated / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
            install_locked_runtime(python)
            installed = subprocess.run(["uv", "pip", "install", "--offline", "--python", str(python), str(wheel)],
                           check=False, capture_output=True, text=True, timeout=30, env=environment)
            self.assertEqual(installed.returncode, 0, installed.stderr)
            program = """import json
from agindow import Agindow
sdk=Agindow()
assert sdk.ready, sdk.load_error
value=object()
assert sdk.guard(lambda:value)() is value
assert not sdk.recent()
print(json.dumps({'ready':sdk.ready}))
sdk.close()
"""
            result = subprocess.run([str(python), "-I", "-c", program], cwd=root,
                                    check=True, capture_output=True, text=True, timeout=30, env=environment)
            self.assertEqual(json.loads(result.stdout), {"ready": True})
