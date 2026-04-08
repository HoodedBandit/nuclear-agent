from __future__ import annotations

import json
import re
import threading
import time
from dataclasses import dataclass
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from .common import read_json


ANALYSIS_MODEL = "analysis-harness-model"
SCRIPTED_MODEL = "scripted-harness-model"
TASK_MARKER_PATTERN = re.compile(r"\[HARNESS_TASK_ID:([A-Za-z0-9._-]+)\]")


def build_analysis_response(prompt: str) -> str:
    lowered = prompt.lower()
    if "return a json map of the workspace crates" in lowered:
        return json.dumps(
            {
                "crates": [
                    {
                        "name": "agent-core",
                        "role": "shared contracts and persisted configuration types",
                        "release_risk": "schema and contract drift across CLI and daemon",
                    },
                    {
                        "name": "agent-daemon",
                        "role": "runtime control plane, tool loop, dashboard, and operator routes",
                        "release_risk": "broad surface area with restart, auth, and connector coupling",
                    },
                    {
                        "name": "agent-providers",
                        "role": "provider adapters and saved credential access",
                        "release_risk": "auth edge cases and provider-specific request translation",
                    },
                    {
                        "name": "agent-storage",
                        "role": "paths, persistence, migration, logs, and plugin state",
                        "release_risk": "upgrade safety and cross-platform path migration behavior",
                    },
                    {
                        "name": "nuclear",
                        "role": "CLI, onboarding, operator commands, and release-facing entrypoint",
                        "release_risk": "broad command surface and installer integration",
                    },
                ],
                "primary_interfaces": [
                    "nuclear CLI command surface",
                    "local HTTP daemon routes under /v1",
                    "dashboard static UI and launch flow",
                    "packaged installers for Windows and Linux",
                ],
                "release_risks": [
                    "provider auth and model access regressions",
                    "managed install migration and rollback safety",
                    "dashboard and daemon recovery across restart paths",
                ],
            }
        )
    if "control-plane transport" in lowered or "control-plane migration status" in lowered:
        return json.dumps(
            {
                "transport": "Authenticated local HTTP control plane with CLI-to-daemon requests and dashboard bootstrap routes.",
                "auth": "Bearer daemon token for API calls plus one-time dashboard launch cookies.",
                "implemented_features": [
                    "daemon status and configuration routes",
                    "session and task execution endpoints",
                    "dashboard launch and bootstrap",
                    "log streaming and operator controls",
                ],
                "remaining_gaps": [
                    "live-account certification remains outside the deterministic coding harness",
                    "soak coverage still depends on a real daemon token and workspace",
                ],
            }
        )
    if "return a json review" in lowered and "dashboard" in lowered:
        return json.dumps(
            {
                "findings": [
                    {
                        "severity": "low",
                        "area": "dashboard bootstrap",
                        "summary": "The dashboard remains tightly coupled to daemon route availability and should keep explicit error handling around launch and reconnect paths.",
                        "evidence": "The UI depends on bootstrap and launch endpoints before session views become usable.",
                    }
                ],
                "residual_risks": [
                    "Live browser signoff is still needed for hosted-provider auth expiry and reconnect behavior.",
                    "Dashboard behavior should continue to be checked after route or asset changes.",
                ],
            }
        )
    if "bounded refactor plan" in lowered or "provider debt-reduction pass" in lowered:
        return json.dumps(
            {
                "target": "crates/agent-providers/src/lib.rs",
                "goal": "Reduce provider-specific branching and keep request translation easier to audit before release.",
                "steps": [
                    {
                        "order": 1,
                        "change": "Extract shared request-building helpers for common chat completion paths.",
                        "verification": "Run provider unit tests and cargo clippy for agent-providers.",
                    },
                    {
                        "order": 2,
                        "change": "Move provider-specific auth and header logic into focused helpers per provider family.",
                        "verification": "Exercise deterministic provider smoke coverage and model listing tests.",
                    },
                    {
                        "order": 3,
                        "change": "Document provider-specific unsupported cases alongside the code paths that enforce them.",
                        "verification": "Review generated release notes and doctor output for consistent operator messaging.",
                    },
                ],
                "tests": [
                    "cargo test -p agent-providers",
                    "cargo clippy --workspace --all-targets --all-features -- -D warnings",
                ],
            }
        )
    return f"Mock reply from {ANALYSIS_MODEL}: {prompt}"


def _message_text(message: dict[str, Any]) -> str:
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, dict) and item.get("type") == "text":
                parts.append(str(item.get("text") or ""))
        return "\n".join(parts)
    return ""


def _extract_task_id(messages: list[dict[str, Any]]) -> str | None:
    for message in reversed(messages):
        if message.get("role") != "user":
            continue
        match = TASK_MARKER_PATTERN.search(_message_text(message))
        if match:
            return match.group(1)
    return None


def _assistant_tool_turns(messages: list[dict[str, Any]]) -> int:
    return sum(
        1
        for message in messages
        if message.get("role") == "assistant" and isinstance(message.get("tool_calls"), list) and message["tool_calls"]
    )


def _recent_tool_outputs(messages: list[dict[str, Any]]) -> list[str]:
    outputs: list[str] = []
    found_last_assistant = False
    for message in reversed(messages):
        role = message.get("role")
        if role == "assistant" and isinstance(message.get("tool_calls"), list) and message["tool_calls"]:
            found_last_assistant = True
            break
        if role == "tool":
            outputs.append(_message_text(message))
    return list(reversed(outputs)) if found_last_assistant else []


def _branch_matches(branch: dict[str, Any], tool_outputs: list[str]) -> bool:
    combined = "\n".join(tool_outputs)
    any_values = [str(value) for value in branch.get("match_any_tool_output_contains") or []]
    all_values = [str(value) for value in branch.get("match_all_tool_output_contains") or []]
    no_values = [str(value) for value in branch.get("match_no_tool_output_contains") or []]
    if any_values and not any(value in combined for value in any_values):
        return False
    if all_values and not all(value in combined for value in all_values):
        return False
    if no_values and any(value in combined for value in no_values):
        return False
    return branch.get("default", False) or bool(any_values or all_values or no_values)


def _select_turn(script: dict[str, Any], turn_index: int, tool_outputs: list[str]) -> dict[str, Any]:
    turns = script.get("turns") or []
    if turn_index >= len(turns):
        raise KeyError(f"turn index {turn_index} exceeds scripted turns")
    turn = dict(turns[turn_index])
    for branch in turn.get("branches") or []:
        if _branch_matches(branch, tool_outputs):
            merged = dict(turn)
            merged.update(branch)
            merged.pop("branches", None)
            return merged
    turn.pop("branches", None)
    return turn


def _normalise_tool_calls(script: dict[str, Any], task_id: str, turn_index: int, turn: dict[str, Any]) -> list[dict[str, Any]]:
    calls: list[dict[str, Any]] = []
    script_root = Path(str(script.get("__script_root__") or ".")).resolve()
    for index, raw_call in enumerate(turn.get("tool_calls") or [], start=1):
        call = dict(raw_call)
        arguments = call.get("arguments")
        if arguments is None and "patch_file" in call:
            patch_path = (script_root / str(call["patch_file"])).resolve()
            arguments = {"patch": patch_path.read_text(encoding="utf-8")}
        if arguments is None:
            raise KeyError(f"tool call in task {task_id} turn {turn_index} is missing arguments")
        calls.append(
            {
                "id": str(call.get("id") or f"{task_id}-turn{turn_index + 1}-call{index}"),
                "type": "function",
                "function": {
                    "name": str(call["name"]),
                    "arguments": json.dumps(arguments, separators=(",", ":")),
                },
            }
        )
    return calls


class HarnessProviderHandler(BaseHTTPRequestHandler):
    server_version = "NuclearHarnessProvider/1.0"

    def _send_json(self, status: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt: str, *args: Any) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802
        if self.path != "/v1/models":
            self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})
            return
        self._send_json(
            HTTPStatus.OK,
            {"data": [{"id": model_id} for model_id in self.server.models]},  # type: ignore[attr-defined]
        )

    def do_POST(self) -> None:  # noqa: N802
        if self.path != "/v1/chat/completions":
            self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})
            return
        length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(length) or b"{}")
        messages = payload.get("messages") or []
        model = str(payload.get("model") or self.server.default_model)  # type: ignore[attr-defined]

        if self.server.mode == "analysis":  # type: ignore[attr-defined]
            prompt = ""
            for message in reversed(messages):
                if message.get("role") == "user":
                    prompt = _message_text(message)
                    break
            content = build_analysis_response(prompt)
            self._send_json(
                HTTPStatus.OK,
                {
                    "id": "chatcmpl-analysis",
                    "object": "chat.completion",
                    "created": int(time.time()),
                    "model": model,
                    "choices": [
                        {
                            "index": 0,
                            "message": {"role": "assistant", "content": content},
                            "finish_reason": "stop",
                        }
                    ],
                },
            )
            return

        task_id = _extract_task_id(messages)
        if not task_id:
            self._send_json(HTTPStatus.BAD_REQUEST, {"error": "missing harness task marker"})
            return
        script = self.server.scripts.get(task_id)  # type: ignore[attr-defined]
        if script is None:
            self._send_json(HTTPStatus.BAD_REQUEST, {"error": f"unknown harness task {task_id}"})
            return

        turn_index = _assistant_tool_turns(messages)
        tool_outputs = _recent_tool_outputs(messages)
        try:
            turn = _select_turn(script, turn_index, tool_outputs)
        except KeyError as error:
            self._send_json(HTTPStatus.BAD_REQUEST, {"error": str(error)})
            return

        if turn.get("tool_calls"):
            tool_calls = _normalise_tool_calls(script, task_id, turn_index, turn)
            message = {"role": "assistant", "content": None, "tool_calls": tool_calls}
            finish_reason = "tool_calls"
        else:
            message = {"role": "assistant", "content": str(turn.get("response") or "")}
            finish_reason = "stop"

        self._send_json(
            HTTPStatus.OK,
            {
                "id": f"chatcmpl-{task_id}-{turn_index + 1}",
                "object": "chat.completion",
                "created": int(time.time()),
                "model": str(script.get("model") or model),
                "choices": [{"index": 0, "message": message, "finish_reason": finish_reason}],
            },
        )


@dataclass
class HarnessProviderServer:
    host: str
    port: int
    mode: str
    default_model: str
    scripts: dict[str, dict[str, Any]] | None = None
    script_root: Path | None = None

    def __post_init__(self) -> None:
        server = ThreadingHTTPServer((self.host, self.port), HarnessProviderHandler)
        server.mode = self.mode  # type: ignore[attr-defined]
        server.default_model = self.default_model  # type: ignore[attr-defined]
        server.scripts = self.scripts or {}  # type: ignore[attr-defined]
        server.script_root = self.script_root or Path.cwd()  # type: ignore[attr-defined]
        models = {self.default_model}
        for script in (self.scripts or {}).values():
            model = script.get("model")
            if isinstance(model, str) and model.strip():
                models.add(model)
        server.models = sorted(models)  # type: ignore[attr-defined]
        self._server = server
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)

    def start(self) -> None:
        self._thread.start()

    def stop(self) -> None:
        self._server.shutdown()
        self._server.server_close()
        self._thread.join(timeout=5)


def load_scripted_turns(task_file: Path, tasks: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    scripts: dict[str, dict[str, Any]] = {}
    for task in tasks:
        script_path_value = task.get("deterministic_script")
        if not script_path_value:
            continue
        script_path = (task_file.parent / str(script_path_value)).resolve()
        script = read_json(script_path)
        script["__script_root__"] = str(script_path.parent)
        scripts[task["id"]] = script
    return scripts
