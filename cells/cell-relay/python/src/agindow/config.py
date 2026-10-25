"""Resolve configuration once; scoped overrides are immutable call snapshots."""
from __future__ import annotations

import json
import os
import warnings
from dataclasses import fields, replace
from pathlib import Path
from typing import Callable, Mapping, cast
from urllib.parse import urlsplit
from .types import AgindowSetupError, Config, Mode


def warn(message: str) -> None:
    try:
        warnings.warn(f"AGINDOW: {message}", RuntimeWarning, stacklevel=2)
    except Exception:
        pass


def validate(config: Config) -> Config:
    if config.mode not in ("observe", "repair", "strict"):
        raise AgindowSetupError("invalid mode")
    if type(config.fail_closed) is not bool:
        raise AgindowSetupError("invalid fail_closed")
    if type(config.buffer) is not int or not 0 <= config.buffer <= 100_000:
        raise AgindowSetupError("invalid buffer (0–100000)")
    if config.on_event is not None and not callable(config.on_event):
        raise AgindowSetupError("invalid on_event")
    if config.api_key is not None and not isinstance(config.api_key, str):
        raise AgindowSetupError("invalid api_key")
    if config.endpoint is not None:
        try:
            url = urlsplit(config.endpoint)
            if url.scheme not in ("http", "https") or not url.netloc:
                raise ValueError("URL")
        except Exception as error:
            raise AgindowSetupError("invalid endpoint") from error
    for collection in (config.allowed_tools, config.deny_tools):
        if collection != "*" and (not isinstance(collection, tuple) or not all(isinstance(item, str) for item in collection)):
            raise AgindowSetupError("invalid tool scope")
    return config


def update(config: Config, changes: Mapping[str, object]) -> Config:
    keys = {field.name for field in fields(Config)}
    if set(changes) - keys:
        raise AgindowSetupError("unknown config field")
    # Dataclass replacement is the sole untyped external-data constructor.
    construct = cast(Callable[..., Config], replace)
    result = construct(config, **changes)
    return validate(result)


def resolve(explicit: Config | None, overrides: Mapping[str, object]) -> Config:
    file_values: dict[str, object] = {}
    directory = Path.cwd()
    for parent in (directory, *directory.parents):
        path = parent / ".agindowrc"
        try:
            raw: object = json.loads(path.read_text())
            if not isinstance(raw, dict):
                raise ValueError("not an object")
            aliases = {"failClosed": "fail_closed", "apiKey": "api_key", "allowedTools": "allowed_tools", "denyTools": "deny_tools"}
            file_values = {aliases.get(key, key): value for key, value in cast(dict[str, object], raw).items()}
            for key in ("allowed_tools", "deny_tools"):
                if isinstance(file_values.get(key), list):
                    file_values[key] = tuple(cast(list[object], file_values[key]))
            update(Config(), file_values)
            break
        except FileNotFoundError:
            continue
        except Exception:
            warn(f"cannot read valid .agindowrc in {parent}; ignoring it")
            file_values = {}
            break
    environment: dict[str, object] = {}
    for key, name in (("mode", "AGINDOW_MODE"), ("api_key", "AGINDOW_KEY"), ("endpoint", "AGINDOW_ENDPOINT")):
        if name in os.environ:
            environment[key] = os.environ[name]
    if "AGINDOW_BUFFER" in os.environ:
        try:
            environment["buffer"] = int(os.environ["AGINDOW_BUFFER"])
        except ValueError as error:
            raise AgindowSetupError("invalid AGINDOW_BUFFER") from error
    if "AGINDOW_FAIL_CLOSED" in os.environ:
        value = os.environ["AGINDOW_FAIL_CLOSED"]
        if value not in ("true", "false"):
            raise AgindowSetupError("invalid AGINDOW_FAIL_CLOSED")
        environment["fail_closed"] = value == "true"
    update(Config(), environment)
    result = update(update(Config(), file_values), environment)
    if explicit is not None:
        # An explicitly constructed Config is a complete configuration.
        result = validate(explicit)
    return update(result, overrides)
