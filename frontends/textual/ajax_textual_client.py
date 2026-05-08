from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from typing import Any, Callable


CommandRunner = Callable[[list[str]], str]


@dataclass(frozen=True)
class AjaxSnapshot:
    repos: list[dict[str, Any]]
    tasks: list[dict[str, Any]]
    review: list[dict[str, Any]]
    inbox: list[dict[str, Any]]


@dataclass(frozen=True)
class AjaxClient:
    ajax_bin: str
    command_runner: CommandRunner = subprocess.check_output

    def json_command(self, *args: str) -> dict[str, Any]:
        command = [self.ajax_bin, *args, "--json"]
        output = self.command_runner(command)
        loaded = json.loads(output)
        if not isinstance(loaded, dict):
            raise ValueError(f"Ajax returned non-object JSON for {' '.join(command)}")
        return loaded

    def snapshot(self) -> AjaxSnapshot:
        loaded = self.json_command("cockpit")
        return AjaxSnapshot(
            repos=list(as_object(loaded.get("repos")).get("repos", [])),
            tasks=list(as_object(loaded.get("tasks")).get("tasks", [])),
            review=list(as_object(loaded.get("review")).get("tasks", [])),
            inbox=list(as_object(loaded.get("inbox")).get("items", [])),
        )


def as_object(value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        return value
    return {}
