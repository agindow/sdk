"""Typed host surface; all drift detection/policy stays in the shared core."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Literal, Protocol

Mode = Literal["observe", "repair", "strict"]
Operation = Literal["read", "write", "unknown"]
DriftClass = Literal["shape", "semantic", "unknown"]
Phase = Literal["request", "response"]
EventType = Literal["bypass", "drift", "blocked", "repair", "error"]


class AgindowSetupError(ValueError):
    """Deterministic configuration misuse, raised before an upstream call."""


@dataclass(frozen=True)
class PolicyFeedback:
    error: Literal["drift_blocked", "drift_halted", "drift_passthrough"]
    tool: str
    drift_class: DriftClass
    drift_id: str
    hint: str
    audit: str


@dataclass(frozen=True)
class VerifiedRepair:
    value: object
    changes: tuple[tuple[str, str], ...] = ()
    verified: bool = True


@dataclass(frozen=True)
class Verdict:
    halt: bool = False
    bypassed: bool = False
    drift_class: DriftClass | None = None
    findings: int = 0
    detectors: int = 0
    malformed: bool = False
    confidence: float | None = None
    audit_id: str | None = None
    feedback: PolicyFeedback | None = None
    repair: VerifiedRepair | None = None


@dataclass(frozen=True)
class Event:
    type: EventType
    tool: str
    mode: Mode
    phase: Phase
    latency_us: float
    verdict: Verdict | None = None
    drift_class: DriftClass | None = None
    confidence: float | None = None
    audit_id: str | None = None
    byte_identical: bool | None = None


@dataclass(frozen=True)
class Config:
    mode: Mode = "observe"
    fail_closed: bool = True
    api_key: str | None = None
    endpoint: str | None = None
    allowed_tools: tuple[str, ...] | Literal["*"] = "*"
    deny_tools: tuple[str, ...] = ()
    buffer: int = 100
    on_event: Callable[[Event], None] | None = None


class EngineCall(Protocol):
    @property
    def request(self) -> Verdict: ...
    def finish(self, value: object) -> Verdict: ...
    def close(self) -> None: ...


class Engine(Protocol):
    def begin(self, value: object, mode: Mode, operation: Operation, fail_closed: bool, tool: str) -> EngineCall: ...


class AgindowDriftError(RuntimeError):
    def __init__(self, tool: str, mode: Mode, phase: Phase, verdict: Verdict, outcome: object = None) -> None:
        super().__init__(f"AGINDOW halted {phase} for {tool}")
        self.tool = tool
        self.mode = mode
        self.phase = phase
        self.verdict = verdict
        self.outcome = outcome
        self.code = "drift_halted" if mode == "strict" else "drift_blocked"
        feedback = verdict.feedback
        self.drift_class = feedback.drift_class if feedback else verdict.drift_class or "unknown"
        self.drift_id = feedback.drift_id if feedback else ""
        self.audit_url = feedback.audit if feedback else ""
        self.hint = feedback.hint if feedback else "Request was not dispatched. Review drift evidence before retrying."
        if phase == "response" and "do not blindly replay" not in self.hint:
            self.hint += " Upstream completed. Inspect the retained outcome; do not blindly replay a write."
