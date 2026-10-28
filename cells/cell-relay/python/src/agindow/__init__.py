"""AGINDOW's typed Python host for the shared Rust/WASM core."""
from __future__ import annotations

from typing import Callable, ParamSpec, TypeVar, overload
from .remote import (RemoteEnvelope as RemoteEnvelope, RemoteOptions as RemoteOptions, RemoteResponse as RemoteResponse,
                     guard_remote as guard_remote, guard_remote_async as guard_remote_async)
from .sdk import Agindow as Agindow, Scope as Scope
from .types import (AgindowDriftError as AgindowDriftError, AgindowSetupError as AgindowSetupError,
                    Config as Config, Event as Event, Mode as Mode, Operation as Operation,
                    PolicyFeedback as PolicyFeedback, Verdict as Verdict, VerifiedRepair as VerifiedRepair)

P = ParamSpec("P")
R = TypeVar("R")
_default: Agindow | None = None


def default() -> Agindow:
    global _default
    if _default is None:
        _default = Agindow()
    return _default


@overload
def guard(fn: Callable[P, R], *, tool: str | None = None, operation: Operation = "read", **options: object) -> Callable[P, R]: ...
@overload
def guard(fn: None = None, *, tool: str | None = None, operation: Operation = "read", **options: object) -> Callable[[Callable[P, R]], Callable[P, R]]: ...
def guard(fn: Callable[P, R] | None = None, *, tool: str | None = None, operation: Operation = "read", **options: object) -> Callable[P, R] | Callable[[Callable[P, R]], Callable[P, R]]:
    if fn is None:
        def decorate(target: Callable[P, R]) -> Callable[P, R]:
            return default().guard(target, tool=tool, operation=operation, **options)
        return decorate
    return default().guard(fn, tool=tool, operation=operation, **options)


def context(**options: object) -> Scope:
    return default().context(**options)


def wrap(fn: Callable[P, R], *, tool: str | None = None, operation: Operation = "read", **options: object) -> Callable[P, R]:
    return default().wrap(fn, tool=tool, operation=operation, **options)


def configure(**options: object) -> None:
    default().configure(**options)


def recent() -> tuple[Event, ...]:
    return default().recent()

from .machine_auth import (RemoteAuth as RemoteAuth, AccessCredential as AccessCredential, ProofSigner as ProofSigner)
from .identity_source import workload_identity as workload_identity

from .selective import (RenameRule as RenameRule, RepairCheck as RepairCheck, SelectiveRepair as SelectiveRepair)
