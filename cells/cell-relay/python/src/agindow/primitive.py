"""Callable lifecycle and availability boundary; decisions come from Engine."""
from __future__ import annotations

import inspect
from dataclasses import replace
from functools import wraps
from math import isfinite
from time import perf_counter
from typing import Callable, ParamSpec, TypeVar, cast
from .types import AgindowDriftError, Config, Engine, EngineCall, Event, EventType, Operation, Phase, Verdict

P = ParamSpec("P")
R = TypeVar("R")


class CallState:
    def __init__(self, config: Config, tool: str, operation: Operation, emit: Callable[[Event], None]) -> None:
        self.config = config
        self.tool = tool
        self.operation = operation
        self.emit = emit
        self.started = perf_counter()
        self.call: EngineCall | None = None
        self.bypassed = False
        self.errored = False
        self.closed = False
        self.request_ok: Callable[[object], bool] = lambda _: True

    def event(self, kind: EventType, phase: Phase, verdict: Verdict | None = None, changed: bool | None = None) -> None:
        safe = replace(verdict, repair=None) if verdict else None
        event = Event(kind, self.tool, self.config.mode, phase, (perf_counter() - self.started) * 1_000_000,
                      safe, verdict.drift_class if verdict else None, verdict.confidence if verdict else None,
                      verdict.audit_id if verdict else None, changed)
        try:
            self.emit(event)
        except Exception:
            pass
        try:
            if self.config.on_event is not None:
                self.config.on_event(event)
        except Exception:
            pass

    def bypass(self, phase: Phase, internal: bool = False) -> None:
        if internal and not self.errored:
            self.errored = True
            self.event("error", phase)
        if not self.bypassed:
            self.bypassed = True
            self.event("bypass", phase)

    def close(self) -> None:
        if self.closed:
            return
        self.closed = True
        try:
            if self.call:
                self.call.close()
        except Exception:
            self.bypass("response", True)

    def apply(self, verdict: Verdict, phase: Phase, value: object, error: BaseException | None = None) -> object:
        if verdict.bypassed:
            self.bypass(phase)
            return value
        halted = verdict.halt and self.config.mode != "observe"
        effective = value
        patch = verdict.repair
        if not halted and patch is not None and self.config.mode != "observe":
            confidence = verdict.confidence
            threshold = .99 if self.config.mode == "strict" or verdict.drift_class == "semantic" else .97
            if (not patch.verified or verdict.drift_class in (None, "unknown") or confidence is None
                    or not isfinite(confidence) or not threshold <= confidence <= 1
                    or phase == "request" and not self.request_ok(patch.value)):
                self.bypass(phase, True)
                return value
            effective = patch.value
        if verdict.findings > 0:
            self.event("drift", phase, verdict)
        if halted:
            self.event("blocked", phase, verdict)
            raise AgindowDriftError(self.tool, self.config.mode, phase, verdict, None if error else value) from error
        if effective is not value:
            self.event("repair", phase, verdict, False)
        return effective

    def begin(self, engine: Engine | None, value: object) -> object:
        try:
            scoped_out = (self.tool in self.config.deny_tools
                          or self.config.allowed_tools != "*" and self.tool not in self.config.allowed_tools)
            if engine is None or scoped_out:
                self.bypass("request")
                return value
            self.call = engine.begin(value, self.config.mode, self.operation, self.config.fail_closed, self.tool)
            verdict = self.call.request
        except Exception:
            self.bypass("request", True)
            return value
        if verdict.halt and not verdict.bypassed and self.config.mode != "observe":
            self.close()
        return value if self.bypassed else self.apply(verdict, "request", value)

    def finish(self, value: object, error: BaseException | None = None) -> object:
        verdict: Verdict | None = None
        if self.call is not None and not self.bypassed:
            try:
                verdict = self.call.finish(error if error is not None else value)
            except Exception:
                self.bypass("response", True)
        self.close()
        effective = value
        if verdict is not None and not self.bypassed:
            effective = self.apply(verdict, "response", value, error)
        if error is not None:
            raise error
        return effective


def wrap_callable(fn: Callable[P, R], snapshot: Callable[[], Config], engine: Callable[[], Engine | None],
                  emit: Callable[[Event], None], tool: str, operation: Operation) -> Callable[P, R]:
    invoke = cast(Callable[..., R], fn)

    def prepare(args: tuple[object, ...], kwargs: dict[str, object]) -> tuple[CallState, tuple[object, ...], dict[str, object]]:
        state = CallState(snapshot(), tool, operation, emit)
        prefix: tuple[object, ...] = ()
        positional = args
        # A real descriptor binding, not a parameter-name heuristic, identifies
        # Python's receiver. It is forwarded but never inspected as payload.
        name: object = getattr(fn, "__name__", None)
        if args and isinstance(name, str):
            receiver = args[0] if isinstance(args[0], type) else type(args[0])
            member: object = inspect.getattr_static(receiver, name, None)
            if member is public or isinstance(member, classmethod) and member.__func__ is public:
                prefix, positional = args[:1], args[1:]
        state.request_ok = lambda replacement: not kwargs and (len(positional) <= 1
            or isinstance(replacement, tuple) and len(replacement) == len(positional))
        original: object = positional[0] if len(positional) == 1 and not kwargs else {"args": positional, "kwargs": kwargs} if kwargs else positional if positional else {}
        effective = state.begin(engine(), original)
        if effective is not original:
            if len(positional) <= 1 and not kwargs:
                args = (*prefix, effective)
            elif isinstance(effective, tuple) and len(effective) == len(positional) and not kwargs:
                args = (*prefix, *effective)
            else:
                state.bypass("request", True)
        return state, args, kwargs

    @wraps(fn)
    def wrapped(*args: P.args, **kwargs: P.kwargs) -> R:
        state, actual_args, actual_kwargs = prepare(args, kwargs)
        try:
            result = invoke(*actual_args, **actual_kwargs)
        except BaseException as error:
            return cast(R, state.finish(None, error))
        if inspect.isawaitable(result):
            async def finish_async() -> object:
                try:
                    value = await result
                except BaseException as error:
                    return state.finish(None, error)
                return state.finish(value)
            return cast(R, finish_async())
        return cast(R, state.finish(result))

    public: Callable[P, R] = wrapped
    if inspect.iscoroutinefunction(fn):
        @wraps(fn)
        async def async_wrapped(*args: P.args, **kwargs: P.kwargs) -> object:
            value = wrapped(*args, **kwargs)
            if inspect.isawaitable(value):
                return await value
            return value
        public = cast(Callable[P, R], async_wrapped)
    return public
