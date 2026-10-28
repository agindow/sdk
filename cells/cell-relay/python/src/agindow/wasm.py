"""Thin wasm-bindgen host ABI for the *same bytes* used by agindow.

Only these three generated imports are linked. No WASI, network or filesystem
capability is given to the guest. The pinned generator's ABI is package-tested.
"""
from __future__ import annotations

import json
import weakref
from pathlib import Path
from threading import RLock
from time import monotonic
from typing import cast
import wasmtime
from .capture import capture
from .selective import RenameRule
from .types import DriftClass, EngineCall, Mode, Operation, Verdict


def _verdict(value: str) -> Verdict:
    parsed: object = json.loads(value)
    if not isinstance(parsed, dict):
        raise ValueError("invalid guest verdict")
    data = cast(dict[str, object], parsed)
    for key in ("halt", "bypassed", "malformed"):
        if type(data.get(key)) is not bool:
            raise ValueError("invalid guest verdict flag")
    for key in ("findings", "detectors"):
        count = data.get(key)
        if type(count) is not int or count < 0:
            raise ValueError("invalid guest verdict count")
    drift = data.get("class")
    if drift not in (None, "shape", "semantic", "unknown"):
        raise ValueError("invalid guest class")
    return Verdict(halt=cast(bool, data["halt"]), bypassed=cast(bool, data["bypassed"]),
                   drift_class=drift, findings=cast(int, data["findings"]),
                   detectors=cast(int, data["detectors"]), malformed=cast(bool, data["malformed"]))


class WasmEngine:
    def __init__(self, *, selection: dict[str, object] | None = None, work: int = 200_000_000,
                 binary: bytes | None = None) -> None:
        if type(work) is not int or not 0 <= work <= 0xFFFFFFFF:
            raise ValueError("invalid work allowance")
        self.lock = RLock()
        self.retired = False
        self.work = work
        self.tool = selection.get("operation") if selection is not None else None
        self.runtime = wasmtime.Engine()
        self.store = wasmtime.Store(self.runtime)
        self._cleanup = weakref.finalize(self, self.store.close)
        self.module = wasmtime.Module(self.runtime, binary if binary is not None else Path(__file__).with_name("_guest.wasm").read_bytes())
        linker = wasmtime.Linker(self.runtime)
        reference = weakref.ref(self)

        def host() -> WasmEngine:
            engine = reference()
            if engine is None:
                raise RuntimeError("guest released")
            return engine

        def read(pointer: int, length: int) -> str:
            return host()._read(pointer, length)

        def throw(pointer: int, length: int) -> None:
            raise RuntimeError(read(pointer, length))

        def init_refs() -> None:
            host()._init_refs()

        namespace = "./agw_node_guest_bg.js"
        for imported in self.module.imports:
            if imported.module != namespace or imported.name is None or not isinstance(imported.type, wasmtime.FuncType):
                raise ValueError("unexpected guest import")
            if imported.name.startswith("__wbg___wbindgen_throw_"):
                linker.define_func(namespace, imported.name, imported.type, throw)
            elif imported.name == "__wbindgen_generic_0000000000000001":
                linker.define_func(namespace, imported.name, imported.type, read)
            elif imported.name == "__wbindgen_init_externref_table":
                linker.define_func(namespace, imported.name, imported.type, init_refs)
            else:
                raise ValueError("unexpected guest import")
        self.instance = linker.instantiate(self.store, self.module)
        self.exports = self.instance.exports(self.store)
        memory = self.exports["memory"]
        refs = self.exports["__wbindgen_externrefs"]
        if not isinstance(memory, wasmtime.Memory) or not isinstance(refs, wasmtime.Table):
            raise ValueError("invalid guest exports")
        self.memory = memory
        self.refs = refs
        self._invoke("__wbindgen_start")
        pointer, length = self._input(json.dumps(selection, separators=(",", ":")))
        result = self._result(self._invoke("guest_new", pointer, length, monotonic() * 1000), 3)
        self._error(result[2], result[1])
        self.pointer = result[0]
        self.active = bool(self._invoke("guest_active", self.pointer))

    def _init_refs(self) -> None:
        offset = self.refs.grow(self.store, 4, None)
        for index, value in enumerate((None, None, True, False)):
            self.refs.set(self.store, offset + index, value)
        self.refs.set(self.store, 0, None)

    def _read(self, pointer: int, length: int) -> str:
        return bytes(self.memory.read(self.store, pointer, pointer + length)).decode("utf-8")

    def _invoke(self, name: str, *args: object) -> object:
        if self.retired:
            raise RuntimeError("guest retired")
        try:
            function = self.exports[name]
            if not isinstance(function, wasmtime.Func):
                raise ValueError("invalid guest function")
            return cast(object, function(self.store, *args))
        except Exception:
            self.retired = True
            raise

    def _input(self, value: str) -> tuple[int, int]:
        data = value.encode("utf-8")
        pointer = self._invoke("__wbindgen_malloc", len(data), 1)
        if not isinstance(pointer, int):
            raise ValueError("invalid guest allocation")
        self.memory.write(self.store, data, pointer)
        # wasm-bindgen consumes this allocation, as in its generated JS glue.
        return pointer, len(data)

    @staticmethod
    def _result(value: object, length: int) -> list[int]:
        if not isinstance(value, list) or len(value) != length or any(type(item) is not int for item in value):
            raise ValueError("invalid guest ABI result")
        return cast(list[int], value)

    def _error(self, flag: int, index: int) -> None:
        if flag:
            value: object = self.refs.get(self.store, index)
            self._invoke("__externref_table_dealloc", index)
            raise RuntimeError(str(value))

    def _output(self, value: object) -> Verdict:
        result = self._result(value, 4)
        self._error(result[3], result[2])
        pointer, length = result[:2]
        try:
            return _verdict(self._read(pointer, length))
        finally:
            self._invoke("__wbindgen_free", pointer, length, 1)

    def prepare_rename(self, before: str, rule: RenameRule, mode: Mode) -> str:
        with self.lock:
            body, body_len = self._input(before)
            source, source_len = self._input(rule.source)
            target, target_len = self._input(rule.target)
            mode_ptr, mode_len = self._input(mode)
            result = self._result(self._invoke("guest_prepare_rename", self.pointer, body, body_len, source, source_len,
                                               target, target_len, rule.confidence, mode_ptr, mode_len, self.work), 4)
            self._error(result[3], result[2])
            try:
                return self._read(result[0], result[1])
            finally:
                self._invoke("__wbindgen_free", result[0], result[1], 1)

    def begin(self, value: object, mode: Mode, operation: Operation, fail_closed: bool, tool: str) -> EngineCall:
        with self.lock:
            if self.retired:
                raise RuntimeError("guest retired")
            if self.tool is not None and self.tool != tool:
                raise ValueError("selection scope mismatch")
            if not self.active:
                result = self._result(self._invoke("guest_null_call", self.pointer,
                    ("observe", "repair", "strict").index(mode), ("read", "write", "unknown").index(operation),
                    int(fail_closed), self.work, monotonic() * 1000), 3)
                self._error(result[2], result[1])
                if result[0] != 1:
                    raise RuntimeError("Empty statistical plan did not complete cleanly")
                return EmptyPlanCall()
            data = capture(value) if self.active else "{}"
            try:
                req, req_len = self._input(data)
                mode_ptr, mode_len = self._input(mode)
                op_ptr, op_len = self._input(operation)
                result = self._result(self._invoke("guest_begin", self.pointer, req, req_len, 0, mode_ptr, mode_len,
                                                  op_ptr, op_len, monotonic() * 1000, int(fail_closed), self.work), 3)
                self._error(result[2], result[1])
                request = self._output(self._invoke("guestcall_request", result[0]))
                return WasmCall(self, result[0], request)
            except Exception:
                self.retired = True
                raise

    def close(self) -> None:
        with self.lock:
            if not self.retired:
                self._invoke("__wbg_guest_free", self.pointer, 0)
            self.retired = True
            self._cleanup()


class WasmCall:
    def __init__(self, engine: WasmEngine, pointer: int, request: Verdict) -> None:
        self.engine = engine
        self.pointer = pointer
        self.request = request
        self.closed = False

    def _finish(self, data: str, truncated: bool) -> Verdict:
        with self.engine.lock:
            if self.closed:
                raise RuntimeError("call already finished")
            self.closed = True
            try:
                pointer, length = self.engine._input(data)
                return self.engine._output(self.engine._invoke("guestcall_finish", self.pointer, pointer, length,
                                                             int(truncated), monotonic() * 1000))
            except Exception:
                self.engine.retired = True
                raise

    def finish(self, value: object) -> Verdict:
        try:
            data = capture(value) if self.engine.active else "{}"
        except Exception:
            verdict = self._finish("{}", True)
            return Verdict(bypassed=True, drift_class=verdict.drift_class)
        return self._finish(data, False)

    def close(self) -> None:
        if not self.closed and not self.engine.retired:
            self._finish("{}", True)


class EmptyPlanCall:
    """The guest already executed both evidence-free empty-plan phases."""
    request = Verdict()

    def finish(self, value: object) -> Verdict:
        return self.request

    def close(self) -> None:
        pass
