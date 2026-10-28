"""Python decorators and task-local scoped configuration."""
from __future__ import annotations

from collections import deque
from contextvars import ContextVar, Token
from dataclasses import replace
from threading import RLock
from types import TracebackType
from typing import Callable, ParamSpec, TypeVar, overload
from .config import resolve, update
from .selective import SelectiveEngine, SelectiveRepair, validate_repair
from .primitive import wrap_callable
from .types import AgindowSetupError, Config, Engine, Event, EventType, Operation

P = ParamSpec("P")
R = TypeVar("R")


class Scope:
    def __init__(self, sdk: Agindow, changes: dict[str, object]) -> None:
        self.sdk = sdk
        self.config = update(sdk._snapshot(), changes)
        self.token: Token[Config | None] | None = None

    def __enter__(self) -> Agindow:
        if self.token is not None:
            raise AgindowSetupError("context is already active")
        self.token = self.sdk._scope.set(self.config)
        return self.sdk

    def __exit__(self, kind: type[BaseException] | None, error: BaseException | None, tb: TracebackType | None) -> None:
        if self.token is not None:
            self.sdk._scope.reset(self.token)
            self.token = None

    async def __aenter__(self) -> Agindow:
        return self.__enter__()

    async def __aexit__(self, kind: type[BaseException] | None, error: BaseException | None, tb: TracebackType | None) -> None:
        self.__exit__(kind, error, tb)


class Agindow:
    def __init__(self, config: Config | None = None, *, engine: Engine | None = None,
                 selection: dict[str, object] | None = None, selective_repair: SelectiveRepair | None = None, work: int = 200_000_000, **overrides: object) -> None:
        repair = validate_repair(selective_repair)
        if repair is not None and engine is not None:
            raise AgindowSetupError("selective_repair requires the bundled engine")
        self._config = resolve(config, overrides)
        self._scope: ContextVar[Config | None] = ContextVar("agindow_scope", default=None)
        self._lock = RLock()
        self._events: deque[Event] = deque(maxlen=self._config.buffer)
        self._listeners: dict[EventType, list[Callable[[Event], None]]] = {}
        self.load_error: Exception | None = None
        if engine is not None:
            self._engine: Engine | None = engine
        else:
            try:
                from .wasm import WasmEngine
                loaded = WasmEngine(selection=selection, work=work if repair is None else work // 3)
                self._engine = loaded if repair is None else SelectiveEngine(loaded, repair, loaded.prepare_rename)
            except Exception as error:
                self.load_error = error
                self._engine = None

    @property
    def config(self) -> Config:
        return self._config

    @property
    def ready(self) -> bool:
        return self._engine is not None

    def _snapshot(self) -> Config:
        return self._scope.get() or self._config

    def configure(self, **changes: object) -> None:
        with self._lock:
            self._config = update(self._config, changes)
            self._events = deque(self._events, maxlen=self._config.buffer)

    def context(self, **changes: object) -> Scope:
        return Scope(self, changes)

    def recent(self) -> tuple[Event, ...]:
        with self._lock:
            return tuple(self._events)

    def on(self, kind: EventType, listener: Callable[[Event], None]) -> Callable[[], None]:
        with self._lock:
            self._listeners.setdefault(kind, []).append(listener)

        def unsubscribe() -> None:
            with self._lock:
                listeners = self._listeners.get(kind, [])
                if listener in listeners:
                    listeners.remove(listener)
        return unsubscribe

    def _emit(self, event: Event) -> None:
        with self._lock:
            self._events.append(event)
            listeners = tuple(self._listeners.get(event.type, []))
        for listener in listeners:
            try:
                listener(event)
            except Exception:
                pass

    @overload
    def guard(self, fn: Callable[P, R], *, tool: str | None = None, operation: Operation = "read", **changes: object) -> Callable[P, R]: ...
    @overload
    def guard(self, fn: None = None, *, tool: str | None = None, operation: Operation = "read", **changes: object) -> Callable[[Callable[P, R]], Callable[P, R]]: ...
    def guard(self, fn: Callable[P, R] | None = None, *, tool: str | None = None, operation: Operation = "read",
              **changes: object) -> Callable[P, R] | Callable[[Callable[P, R]], Callable[P, R]]:
        update(self._snapshot(), changes)
        if operation not in ("read", "write", "unknown") or tool is not None and (not isinstance(tool, str) or not tool):
            raise AgindowSetupError("invalid operation or tool")

        def decorate(target: Callable[P, R]) -> Callable[P, R]:
            if not callable(target):
                raise AgindowSetupError("guard requires a callable")
            candidate: object = getattr(target, "__name__", "tool")
            name = tool or (candidate if isinstance(candidate, str) and candidate else "tool")
            return wrap_callable(target, lambda: update(self._snapshot(), changes), lambda: self._engine, self._emit, name, operation)
        return decorate if fn is None else decorate(fn)

    def wrap(self, target: Callable[P, R], *, tool: str | None = None, operation: Operation = "read", **changes: object) -> Callable[P, R]:
        """The Python primitive accepts callables; framework adapters are separate."""
        return self.guard(target, tool=tool, operation=operation, **changes)

    def close(self) -> None:
        engine = self._engine
        self._engine = None
        if engine is not None:
            close = getattr(engine, "close", None)
            if callable(close):
                try:
                    close()
                except Exception:
                    pass
