from __future__ import annotations

import fnmatch
import json
from pathlib import Path
from typing import Any

from .common import read_json


REQUIRED_CODING_FIELDS = {
    "id",
    "fixture",
    "suite",
    "prompt",
    "setup_commands",
    "success_commands",
    "allowed_change_globs",
    "forbidden_change_globs",
    "max_duration_seconds",
    "max_tool_calls",
    "required_tools",
    "requires_network",
    "requires_shell",
    "requires_edit",
    "final_response_assertions",
}


def _normalise_command_spec(raw_command: dict[str, Any], *, field_name: str, task_id: str, index: int) -> dict[str, Any]:
    if not isinstance(raw_command, dict):
        raise SystemExit(f"{field_name}[{index}] for task '{task_id}' must be an object.")
    argv = raw_command.get("argv")
    if not isinstance(argv, list) or not argv or not all(isinstance(item, str) and item.strip() for item in argv):
        raise SystemExit(f"{field_name}[{index}] for task '{task_id}' must include a non-empty argv array.")
    return {
        "label": str(raw_command.get("label") or f"{field_name}-{index + 1}"),
        "argv": [str(item) for item in argv],
        "expected_exit_code": int(raw_command.get("expected_exit_code", 0)),
        "workdir": str(raw_command.get("workdir") or "."),
        "timeout_seconds": float(raw_command.get("timeout_seconds", 120.0)),
    }


def _normalise_command_list(task: dict[str, Any], field_name: str) -> list[dict[str, Any]]:
    raw_commands = task.get(field_name)
    if not isinstance(raw_commands, list):
        raise SystemExit(f"{field_name} for task '{task.get('id')}' must be a list.")
    return [
        _normalise_command_spec(raw_command, field_name=field_name, task_id=str(task["id"]), index=index)
        for index, raw_command in enumerate(raw_commands)
    ]


def load_analysis_tasks(task_file: Path) -> list[dict[str, Any]]:
    tasks: list[dict[str, Any]] = []
    for raw_line in task_file.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        task = json.loads(line)
        task_id = str(task.get("id") or "").strip()
        if not task_id:
            raise SystemExit(f"Analysis task is missing id: {raw_line}")
        command = [str(value) for value in task.get("command") or []]
        if not command:
            raise SystemExit(f"Analysis task '{task_id}' is missing command arguments.")
        task["id"] = task_id
        task["command"] = command
        task["description"] = str(task.get("description") or "")
        task["category"] = str(task.get("category") or "")
        task["tags"] = [str(value) for value in task.get("tags") or []]
        task["expected_exit_code"] = int(task.get("expected_exit_code", 0))
        task["cwd"] = str(task.get("cwd") or ".")
        tasks.append(task)
    return tasks


def load_coding_tasks(task_file: Path) -> list[dict[str, Any]]:
    if task_file.suffix.lower() == ".jsonl":
        raise SystemExit(
            f"Coding task manifests must be JSON files, not JSONL task lists: {task_file}. "
            "Use harness/tasks/coding/tasks.json for coding lanes or run the analysis-smoke lane for JSONL benchmark tasks."
        )
    payload = read_json(task_file)
    raw_tasks = payload.get("tasks")
    if not isinstance(raw_tasks, list) or not raw_tasks:
        raise SystemExit(f"Coding task manifest at {task_file} must include a non-empty tasks array.")
    tasks: list[dict[str, Any]] = []
    for raw_task in raw_tasks:
        if not isinstance(raw_task, dict):
            raise SystemExit(f"Every coding task entry in {task_file} must be an object.")
        missing = sorted(REQUIRED_CODING_FIELDS - set(raw_task.keys()))
        if missing:
            raise SystemExit(f"Coding task '{raw_task.get('id')}' is missing required fields: {', '.join(missing)}")
        task = dict(raw_task)
        task_id = str(task["id"]).strip()
        if not task_id:
            raise SystemExit("Coding tasks must include a non-empty id.")
        task["id"] = task_id
        task["fixture"] = str(task["fixture"])
        task["suite"] = str(task["suite"])
        task["prompt"] = str(task["prompt"])
        task["setup_commands"] = _normalise_command_list(task, "setup_commands")
        task["precondition_commands"] = _normalise_command_list(task, "precondition_commands") if "precondition_commands" in task else []
        task["expected_failure_before_run"] = (
            _normalise_command_list(task, "expected_failure_before_run") if "expected_failure_before_run" in task else []
        )
        task["success_commands"] = _normalise_command_list(task, "success_commands")
        task["post_run_assertions"] = (
            _normalise_command_list(task, "post_run_assertions") if "post_run_assertions" in task else []
        )
        task["cleanup_commands"] = _normalise_command_list(task, "cleanup_commands") if "cleanup_commands" in task else []
        task["allowed_change_globs"] = [str(value) for value in task.get("allowed_change_globs") or []]
        task["forbidden_change_globs"] = [str(value) for value in task.get("forbidden_change_globs") or []]
        task["expected_changed_paths"] = [str(value) for value in task.get("expected_changed_paths") or []]
        task["required_tools"] = [str(value) for value in task.get("required_tools") or []]
        task["max_duration_seconds"] = int(task["max_duration_seconds"])
        task["max_tool_calls"] = int(task["max_tool_calls"])
        task["requires_network"] = bool(task["requires_network"])
        task["requires_shell"] = bool(task["requires_shell"])
        task["requires_edit"] = bool(task["requires_edit"])
        task["deterministic_script"] = str(task.get("deterministic_script") or "")
        assertions = task.get("final_response_assertions")
        if not isinstance(assertions, dict):
            raise SystemExit(f"Task '{task_id}' final_response_assertions must be an object.")
        task["final_response_assertions"] = {
            "contains_all": [str(value) for value in assertions.get("contains_all") or []],
            "contains_any": [str(value) for value in assertions.get("contains_any") or []],
            "not_contains": [str(value) for value in assertions.get("not_contains") or []],
        }
        tasks.append(task)
    return tasks


def load_provider_profile(profile_path: Path) -> dict[str, Any]:
    profile = read_json(profile_path)
    if not isinstance(profile, dict):
        raise SystemExit(f"Provider profile at {profile_path} must be a JSON object.")
    allowed_keys = {"alias", "provider_id", "model", "provider_kind", "base_url", "api_key_env"}
    unknown = sorted(set(profile.keys()) - allowed_keys)
    if unknown:
        raise SystemExit(f"Provider profile at {profile_path} contains unknown keys: {', '.join(unknown)}")
    return {key: str(value) for key, value in profile.items() if value not in (None, "")}


def merge_provider_profile(base_profile: dict[str, Any] | None, overrides: dict[str, str]) -> dict[str, Any]:
    merged = dict(base_profile or {})
    for key, value in overrides.items():
        if value:
            merged[key] = value
    return merged


def select_tasks(tasks: list[dict[str, Any]], task_filter: str) -> list[dict[str, Any]]:
    if not task_filter.strip():
        return tasks
    patterns = [pattern.strip() for pattern in task_filter.split(",") if pattern.strip()]
    selected: list[dict[str, Any]] = []
    for task in tasks:
        task_id = str(task["id"])
        suite = str(task.get("suite") or "")
        if any(
            fnmatch.fnmatchcase(task_id, pattern)
            or fnmatch.fnmatchcase(suite, pattern)
            or task_id == pattern
            for pattern in patterns
        ):
            selected.append(task)
    return selected
