"""Stage the exact shared WASM artifact and build independently installable distributions."""
from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
from pathlib import Path

PROJECT = Path(__file__).resolve().parents[1]
NODE = PROJECT.parent / "node"
ROOT = PROJECT.parents[2]


def stage() -> str:
    source = NODE / "dist/guest/agw_node_guest_bg.wasm"
    if not source.is_file():
        raise RuntimeError("Build the Node shared guest first: node cells/cell-relay/node/scripts/build.mjs")
    destination = PROJECT / "src/agindow/_guest.wasm"
    shutil.copyfile(source, destination)
    for name in ("LICENSE", "NOTICE"):
        shutil.copyfile(ROOT / name, PROJECT / name)
    return hashlib.sha256(source.read_bytes()).hexdigest()


if __name__ == "__main__":
    print(f"shared WASM sha256: {stage()}", flush=True)
    if "--stage-only" not in sys.argv:
        subprocess.run(["uv", "build", "--no-sources"], cwd=PROJECT, check=True)
