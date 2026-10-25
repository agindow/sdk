"""Minimal observe-mode example — installs from PyPI."""

from agindow import Agindow, guard


@guard(mode="observe", tool="example.read", operation="read")
def read_example() -> dict[str, bool]:
    return {"ok": True}


def main() -> None:
    sdk = Agindow(mode="observe")
    try:
        print({"ready": sdk.ready, "result": read_example()})
    finally:
        sdk.close()


if __name__ == "__main__":
    main()
