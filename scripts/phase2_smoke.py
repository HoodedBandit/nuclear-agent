#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import urllib.request
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


DAEMON_TOKEN = "phase2-smoke-token"
DEFAULT_COMMAND_TIMEOUT = 120.0
LOG_PATH: Path | None = None

PRIMARY_PROVIDER_ID = "local-codex"
SECONDARY_PROVIDER_ID = "local-claude"
PRIMARY_MODEL = "mock-codex"
SECONDARY_MODEL = "mock-claude"
PLUGIN_ID = "echo-toolkit"
INBOX_ID = "phase2-inbox"
MCP_ID = "phase2-mcp"
APP_ID = "phase2-app"
MEMORY_SUBJECT = "Phase 2 Memory"
MEMORY_CONTENT = "Operator surface smoke memory."
PROMPT_TEXT = "Phase 2 provider path"


class MockProviderHandler(BaseHTTPRequestHandler):
    server_version = "Phase2MockProvider/1.0"

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
        if self.path == "/v1/models":
            self._send_json(
                HTTPStatus.OK,
                {"data": [{"id": PRIMARY_MODEL}, {"id": SECONDARY_MODEL}]},
            )
            return
        self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        if self.path != "/v1/chat/completions":
            self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})
            return

        length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(length) or b"{}")
        messages = payload.get("messages") or []
        prompt = "empty"
        for message in reversed(messages):
            if message.get("role") != "user":
                continue
            content = message.get("content")
            if isinstance(content, str):
                prompt = content
                break
            if isinstance(content, list):
                text_parts = [
                    item.get("text", "")
                    for item in content
                    if isinstance(item, dict) and item.get("type") == "text"
                ]
                if text_parts:
                    prompt = "\n".join(text_parts)
                    break
        model = payload.get("model") or PRIMARY_MODEL
        self._send_json(
            HTTPStatus.OK,
            {
                "id": "chatcmpl-phase2",
                "object": "chat.completion",
                "created": int(time.time()),
                "model": model,
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": f"Mock reply from {model}: {prompt}",
                        },
                        "finish_reason": "stop",
                    }
                ],
            },
        )


class MockProviderServer:
    def __init__(self, host: str, port: int) -> None:
        self._server = ThreadingHTTPServer((host, port), MockProviderHandler)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)

    def start(self) -> None:
        self._thread.start()

    def stop(self) -> None:
        self._server.shutdown()
        self._server.server_close()
        self._thread.join(timeout=5)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Phase 2 isolated operator surface smoke test")
    parser.add_argument("--binary-path", required=True)
    parser.add_argument("--repo-root", required=True)
    parser.add_argument("--scenario-root", required=True)
    parser.add_argument("--daemon-port", type=int, default=42911)
    parser.add_argument("--provider-port", type=int, default=42912)
    return parser.parse_args()


def with_suffix(text: str, *, suffix: str) -> str:
    return f"{text}\n{suffix}" if not text.endswith("\n") else f"{text}{suffix}"


def log_step(message: str) -> None:
    line = f"[phase2] {message}"
    print(line, flush=True)
    if LOG_PATH is not None:
        with LOG_PATH.open("a", encoding="utf-8") as handle:
            handle.write(f"{line}\n")


def run_command(
    args: list[str],
    *,
    env: dict[str, str],
    cwd: Path,
    check: bool = True,
    timeout: float = DEFAULT_COMMAND_TIMEOUT,
    capture_output: bool = True,
) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(
            args,
            cwd=str(cwd),
            env=env,
            text=True,
            stdout=subprocess.PIPE if capture_output else subprocess.DEVNULL,
            stderr=subprocess.PIPE if capture_output else subprocess.DEVNULL,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(
            with_suffix(
                f"command timed out after {timeout:.1f}s: {' '.join(args)}",
                suffix=f"stdout:\n{exc.stdout or ''}\nstderr:\n{exc.stderr or ''}",
            )
        ) from exc
    if check and result.returncode != 0:
        raise RuntimeError(
            with_suffix(
                f"command failed ({result.returncode}): {' '.join(args)}",
                suffix=f"stdout:\n{result.stdout or ''}\nstderr:\n{result.stderr or ''}",
            )
        )
    return result


def parse_key_value_output(output: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw_line in output.splitlines():
        line = raw_line.strip()
        if not line or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def request_json(
    method: str,
    url: str,
    *,
    body: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
) -> Any:
    data = None
    request_headers = dict(headers or {})
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        request_headers.setdefault("Content-Type", "application/json")
    request = urllib.request.Request(url, data=data, headers=request_headers, method=method)
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read().decode("utf-8"))


def wait_for_http_json(url: str, *, headers: dict[str, str] | None = None, timeout: float = 30.0) -> Any:
    deadline = time.time() + timeout
    while True:
        try:
            return request_json("GET", url, headers=headers)
        except Exception:
            if time.time() >= deadline:
                raise
            time.sleep(0.3)


def wait_for_daemon_down(url: str, *, headers: dict[str, str] | None = None, timeout: float = 30.0) -> None:
    deadline = time.time() + timeout
    while True:
        try:
            request_json("GET", url, headers=headers)
        except Exception:
            return
        if time.time() >= deadline:
            raise RuntimeError("daemon did not stop in time")
        time.sleep(0.3)


def assert_in(text: str, needle: str, *, context: str) -> None:
    if needle not in text:
        raise RuntimeError(f"expected {needle!r} in {context}\n{text}")


def assert_true(condition: bool, *, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def configure_profile_env(scenario_root: Path) -> dict[str, str]:
    env = os.environ.copy()
    if os.name == "nt":
        user_profile = scenario_root / "UserProfile"
        appdata = scenario_root / "AppData" / "Roaming"
        local_appdata = scenario_root / "AppData" / "Local"
        for path in [user_profile, appdata, local_appdata]:
            path.mkdir(parents=True, exist_ok=True)
        env["USERPROFILE"] = str(user_profile)
        env["APPDATA"] = str(appdata)
        env["LOCALAPPDATA"] = str(local_appdata)
        env["HOME"] = str(user_profile)
    else:
        home = scenario_root / "home"
        xdg_config = scenario_root / "xdg-config"
        xdg_data = scenario_root / "xdg-data"
        xdg_state = scenario_root / "xdg-state"
        for path in [home, xdg_config, xdg_data, xdg_state]:
            path.mkdir(parents=True, exist_ok=True)
        env["HOME"] = str(home)
        env["XDG_CONFIG_HOME"] = str(xdg_config)
        env["XDG_DATA_HOME"] = str(xdg_data)
        env["XDG_STATE_HOME"] = str(xdg_state)
    return env


def load_config(config_path: Path) -> dict[str, Any]:
    return json.loads(config_path.read_text(encoding="utf-8"))


def save_config(config_path: Path, config: dict[str, Any]) -> None:
    config_path.write_text(json.dumps(config, indent=2), encoding="utf-8")


def update_base_config(config_path: Path, repo_root: Path, daemon_port: int) -> None:
    config = load_config(config_path)
    config["daemon"]["host"] = "127.0.0.1"
    config["daemon"]["port"] = daemon_port
    config["daemon"]["token"] = DAEMON_TOKEN
    config["daemon"]["auto_start"] = False
    config["trust_policy"]["trusted_paths"] = [str(repo_root)]
    config["permission_preset"] = "suggest"
    config["onboarding_complete"] = False
    save_config(config_path, config)


def set_onboarding_complete(config_path: Path, *, value: bool) -> None:
    config = load_config(config_path)
    config["onboarding_complete"] = value
    save_config(config_path, config)


def append_file(path: Path, text: str) -> None:
    with path.open("a", encoding="utf-8") as handle:
        handle.write(text)


def parse_named_value(text: str, key: str) -> str:
    match = re.search(rf"{re.escape(key)}=([^\s]+)", text)
    if not match:
        raise RuntimeError(f"could not find {key}=... in output\n{text}")
    return match.group(1)


def command_failed_output(result: subprocess.CompletedProcess[str]) -> str:
    return f"{result.stdout}\n{result.stderr}"


def main() -> int:
    args = parse_args()
    binary_path = Path(args.binary_path).resolve()
    repo_root = Path(args.repo_root).resolve()
    scenario_root = Path(args.scenario_root).resolve()
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    scenario_root.mkdir(parents=True, exist_ok=True)

    global LOG_PATH
    LOG_PATH = scenario_root / "phase2-smoke.log"
    LOG_PATH.write_text("", encoding="utf-8")

    env = configure_profile_env(scenario_root)
    base_url = f"http://127.0.0.1:{args.daemon_port}"
    status_url = f"{base_url}/v1/status"
    auth_headers = {"Authorization": f"Bearer {DAEMON_TOKEN}"}

    fixtures_root = scenario_root / "fixtures"
    plugin_source = fixtures_root / "echo-plugin"
    inbox_dir = fixtures_root / "inbox"
    schema_path = fixtures_root / "tool-schema.json"
    shutil.copytree(repo_root / "tests" / "dashboard-e2e" / "fixtures" / "echo-plugin", plugin_source)
    inbox_dir.mkdir(parents=True, exist_ok=True)
    schema_path.write_text(
        json.dumps(
            {
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    server = MockProviderServer("127.0.0.1", args.provider_port)
    log_step(f"starting mock provider on 127.0.0.1:{args.provider_port}")
    server.start()

    try:
        wait_for_http_json(f"http://127.0.0.1:{args.provider_port}/v1/models")

        log_step("bootstrapping isolated profile")
        doctor = run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        doctor_values = parse_key_value_output(doctor.stdout)
        config_path = Path(doctor_values["config_path"])
        update_base_config(config_path, repo_root, args.daemon_port)

        provider_base_url = f"http://127.0.0.1:{args.provider_port}/v1"
        log_step("configuring local providers and aliases")
        run_command(
            [
                str(binary_path),
                "provider",
                "add-local",
                "--id",
                PRIMARY_PROVIDER_ID,
                "--name",
                "Local Codex",
                "--kind",
                "openai-compatible",
                "--base-url",
                provider_base_url,
                "--model",
                PRIMARY_MODEL,
            ],
            env=env,
            cwd=repo_root,
        )
        run_command(
            [
                str(binary_path),
                "provider",
                "add-local",
                "--id",
                SECONDARY_PROVIDER_ID,
                "--name",
                "Local Claude",
                "--kind",
                "openai-compatible",
                "--base-url",
                provider_base_url,
                "--model",
                SECONDARY_MODEL,
            ],
            env=env,
            cwd=repo_root,
        )
        run_command(
            [
                str(binary_path),
                "alias",
                "add",
                "--alias",
                "claude",
                "--provider",
                SECONDARY_PROVIDER_ID,
                "--model",
                SECONDARY_MODEL,
                "--description",
                "Phase 2 secondary alias",
            ],
            env=env,
            cwd=repo_root,
        )
        set_onboarding_complete(config_path, value=True)

        log_step("verifying pre-daemon doctor output")
        doctor_ready = run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        assert_in(doctor_ready.stdout, "daemon_running=false", context="doctor output before daemon start")
        assert_in(doctor_ready.stdout, f"{PRIMARY_PROVIDER_ID} ok=true", context="doctor output before daemon start")
        assert_in(doctor_ready.stdout, f"{SECONDARY_PROVIDER_ID} ok=true", context="doctor output before daemon start")

        log_step("starting daemon")
        run_command(
            [str(binary_path), "daemon", "start"],
            env=env,
            cwd=repo_root,
            capture_output=False,
        )
        wait_for_http_json(status_url, headers=auth_headers)

        log_step("verifying provider and alias surfaces")
        provider_list = run_command([str(binary_path), "provider", "list"], env=env, cwd=repo_root)
        assert_in(provider_list.stdout, PRIMARY_PROVIDER_ID, context="provider list")
        assert_in(provider_list.stdout, SECONDARY_PROVIDER_ID, context="provider list")

        alias_list = run_command([str(binary_path), "alias", "list"], env=env, cwd=repo_root)
        assert_in(alias_list.stdout, f"main -> {PRIMARY_PROVIDER_ID} / {PRIMARY_MODEL}", context="alias list")
        assert_in(alias_list.stdout, f"claude -> {SECONDARY_PROVIDER_ID} / {SECONDARY_MODEL}", context="alias list")

        model_list = run_command(
            [str(binary_path), "model", "list", "--provider", PRIMARY_PROVIDER_ID],
            env=env,
            cwd=repo_root,
        )
        assert_in(model_list.stdout, PRIMARY_MODEL, context="model list")
        assert_in(model_list.stdout, SECONDARY_MODEL, context="model list")

        provider_run = run_command(
            [str(binary_path), "exec", "--json", "--alias", "claude", "--mode", "build", PROMPT_TEXT],
            env=env,
            cwd=repo_root,
        )
        run_event = json.loads(provider_run.stdout.strip().splitlines()[-1])
        assert_true(run_event.get("event") == "response", message=f"unexpected exec event: {run_event}")
        assert_true(run_event["alias"] == "claude", message="exec did not use the secondary alias")
        assert_true(run_event["provider_id"] == SECONDARY_PROVIDER_ID, message="exec did not target the secondary provider")
        assert_true(run_event["model"] == SECONDARY_MODEL, message="exec did not persist the secondary model")
        assert_true(PROMPT_TEXT in run_event["response"], message="provider response did not include the prompt")
        session_id = run_event["session_id"]

        log_step("updating permissions and trust through the operator surfaces")
        permissions = run_command([str(binary_path), "permissions", "auto-edit"], env=env, cwd=repo_root)
        assert_in(permissions.stdout, "permission_preset=auto-edit", context="permissions output")
        trust = run_command(
            [
                str(binary_path),
                "trust",
                "--path",
                str(repo_root),
                "--allow-shell",
                "true",
                "--allow-network",
                "true",
                "--allow-full-disk",
                "false",
                "--allow-self-edit",
                "false",
            ],
            env=env,
            cwd=repo_root,
        )
        assert_in(trust.stdout, "shell=true", context="trust output")
        assert_in(trust.stdout, "network=true", context="trust output")

        log_step("verifying plugin lifecycle and doctor surfaces")
        install_plugin = run_command(
            [str(binary_path), "plugin", "install", "--trust", str(plugin_source)],
            env=env,
            cwd=repo_root,
        )
        assert_in(install_plugin.stdout, f"installed plugin={PLUGIN_ID}", context="plugin install output")

        plugin_list = json.loads(
            run_command(
                [str(binary_path), "plugin", "list", "--json"],
                env=env,
                cwd=repo_root,
            ).stdout
        )
        plugin = next((entry for entry in plugin_list if entry["id"] == PLUGIN_ID), None)
        assert_true(plugin is not None, message="plugin list did not include the installed plugin")
        assert_true(plugin["enabled"] is True, message="plugin should start enabled")
        assert_true(plugin["trusted"] is True, message="plugin should start trusted")

        plugin_doctor = json.loads(
            run_command(
                [str(binary_path), "plugin", "doctor", "--json"],
                env=env,
                cwd=repo_root,
            ).stdout
        )
        report = next((entry for entry in plugin_doctor if entry["id"] == PLUGIN_ID), None)
        assert_true(report is not None, message="plugin doctor did not report the installed plugin")
        assert_true(report["ok"] is True, message=f"plugin doctor should be ready: {report}")
        assert_true(report["runtime_ready"] is True, message=f"plugin runtime should be ready: {report}")

        append_file(plugin_source / "tool.js", f"\n// phase2 update {int(time.time())}\n")
        update_plugin = run_command(
            [str(binary_path), "plugin", "update", PLUGIN_ID],
            env=env,
            cwd=repo_root,
        )
        assert_in(update_plugin.stdout, f"updated plugin={PLUGIN_ID}", context="plugin update output")

        plugin_after_update = json.loads(
            run_command(
                [str(binary_path), "plugin", "doctor", "--json"],
                env=env,
                cwd=repo_root,
            ).stdout
        )
        updated_report = next((entry for entry in plugin_after_update if entry["id"] == PLUGIN_ID), None)
        assert_true(updated_report is not None, message="plugin doctor missing plugin after update")
        assert_true(updated_report["ok"] is False, message="plugin doctor should require a fresh trust review after update")
        assert_in(updated_report["detail"], "review", context="plugin doctor detail after update")

        run_command([str(binary_path), "plugin", "trust", PLUGIN_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "plugin", "disable", PLUGIN_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "plugin", "enable", PLUGIN_ID], env=env, cwd=repo_root)
        plugin_ready_again = json.loads(
            run_command(
                [str(binary_path), "plugin", "doctor", "--json"],
                env=env,
                cwd=repo_root,
            ).stdout
        )
        ready_report = next((entry for entry in plugin_ready_again if entry["id"] == PLUGIN_ID), None)
        assert_true(ready_report is not None and ready_report["ok"] is True, message="plugin doctor did not recover after trust review")

        log_step("verifying inbox connector configuration and recovery")
        run_command(
            [
                str(binary_path),
                "inbox",
                "add",
                "--id",
                INBOX_ID,
                "--name",
                "Phase 2 Inbox",
                "--description",
                "Phase 2 inbox connector",
                "--path",
                str(inbox_dir),
                "--alias",
                "main",
            ],
            env=env,
            cwd=repo_root,
        )
        inbox_list = run_command([str(binary_path), "inbox", "list"], env=env, cwd=repo_root)
        assert_in(inbox_list.stdout, INBOX_ID, context="inbox list")
        run_command([str(binary_path), "inbox", "disable", INBOX_ID], env=env, cwd=repo_root)
        inbox_disabled = run_command([str(binary_path), "inbox", "get", INBOX_ID], env=env, cwd=repo_root)
        assert_in(inbox_disabled.stdout, "enabled=false", context="disabled inbox config")
        run_command([str(binary_path), "inbox", "enable", INBOX_ID], env=env, cwd=repo_root)
        inbox_enabled = run_command([str(binary_path), "inbox", "get", INBOX_ID], env=env, cwd=repo_root)
        assert_in(inbox_enabled.stdout, "enabled=true", context="enabled inbox config")
        (inbox_dir / "phase2-message.txt").write_text("phase2 inbox payload", encoding="utf-8")
        inbox_poll = run_command([str(binary_path), "inbox", "poll", INBOX_ID], env=env, cwd=repo_root)
        assert_in(inbox_poll.stdout, "processed_files=1", context="inbox poll output")

        log_step("verifying memory and mission operator flows")
        remember = run_command(
            [str(binary_path), "memory", "remember", MEMORY_SUBJECT, MEMORY_CONTENT],
            env=env,
            cwd=repo_root,
        )
        memory_id = parse_named_value(remember.stdout, "memory")
        search_memory = run_command(
            [str(binary_path), "memory", "search", MEMORY_SUBJECT],
            env=env,
            cwd=repo_root,
        )
        assert_in(search_memory.stdout, MEMORY_SUBJECT, context="memory search output")

        rebuild_memory = run_command(
            [str(binary_path), "memory", "rebuild", "--session-id", session_id],
            env=env,
            cwd=repo_root,
        )
        assert_in(rebuild_memory.stdout, "sessions_scanned=", context="memory rebuild output")

        add_mission = run_command(
            [
                str(binary_path),
                "mission",
                "add",
                "Phase 2 scheduled mission",
                "--details",
                "operator flow check",
                "--after-seconds",
                "600",
            ],
            env=env,
            cwd=repo_root,
        )
        mission_id = parse_named_value(add_mission.stdout, "mission")
        assert_in(add_mission.stdout, "status=Scheduled", context="mission add output")
        list_missions = run_command([str(binary_path), "mission", "list"], env=env, cwd=repo_root)
        assert_in(list_missions.stdout, mission_id, context="mission list output")
        run_command(
            [str(binary_path), "mission", "pause", mission_id, "--note", "phase2 pause"],
            env=env,
            cwd=repo_root,
        )
        resume_mission = run_command(
            [
                str(binary_path),
                "mission",
                "resume",
                mission_id,
                "--after-seconds",
                "600",
                "--note",
                "phase2 resume",
            ],
            env=env,
            cwd=repo_root,
        )
        assert_in(resume_mission.stdout, "status=Scheduled", context="mission resume output")
        cancel_mission = run_command(
            [str(binary_path), "mission", "cancel", mission_id, "--note", "phase2 cancel"],
            env=env,
            cwd=repo_root,
        )
        assert_in(cancel_mission.stdout, "status=Cancelled", context="mission cancel output")

        log_step("verifying mcp and app connector configuration surfaces")
        tool_command = sys.executable
        run_command(
            [
                str(binary_path),
                "mcp",
                "add",
                "--id",
                MCP_ID,
                "--name",
                "Phase 2 MCP",
                "--description",
                "Phase 2 MCP server",
                "--command",
                tool_command,
                "--tool-name",
                "phase2_mcp_tool",
                "--schema-file",
                str(schema_path),
            ],
            env=env,
            cwd=repo_root,
        )
        run_command(
            [
                str(binary_path),
                "app",
                "add",
                "--id",
                APP_ID,
                "--name",
                "Phase 2 App",
                "--description",
                "Phase 2 app connector",
                "--command",
                tool_command,
                "--tool-name",
                "phase2_app_tool",
                "--schema-file",
                str(schema_path),
            ],
            env=env,
            cwd=repo_root,
        )
        mcp_list = json.loads(
            run_command([str(binary_path), "mcp", "list", "--json"], env=env, cwd=repo_root).stdout
        )
        app_list = json.loads(
            run_command([str(binary_path), "app", "list", "--json"], env=env, cwd=repo_root).stdout
        )
        assert_true(any(entry["id"] == MCP_ID and entry["enabled"] for entry in mcp_list), message="MCP list missing enabled phase 2 entry")
        assert_true(any(entry["id"] == APP_ID and entry["enabled"] for entry in app_list), message="App list missing enabled phase 2 entry")
        run_command([str(binary_path), "mcp", "disable", MCP_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "mcp", "enable", MCP_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "app", "disable", APP_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "app", "enable", APP_ID], env=env, cwd=repo_root)

        log_step("verifying autonomy, autopilot, evolve, and logs surfaces")
        autonomy_status = run_command([str(binary_path), "autonomy", "status"], env=env, cwd=repo_root)
        assert_in(autonomy_status.stdout, "state=disabled", context="autonomy status output")

        autopilot_config = run_command(
            [
                str(binary_path),
                "autopilot",
                "config",
                "--interval-seconds",
                "45",
                "--max-concurrent",
                "2",
                "--allow-shell",
                "true",
                "--allow-network",
                "true",
                "--allow-self-edit",
                "false",
            ],
            env=env,
            cwd=repo_root,
        )
        assert_in(autopilot_config.stdout, "interval=45s", context="autopilot config output")
        assert_in(autopilot_config.stdout, "concurrency=2", context="autopilot config output")
        run_command([str(binary_path), "autopilot", "enable"], env=env, cwd=repo_root)
        run_command([str(binary_path), "autopilot", "pause"], env=env, cwd=repo_root)
        autopilot_resume = run_command([str(binary_path), "autopilot", "resume"], env=env, cwd=repo_root)
        assert_in(autopilot_resume.stdout, "autopilot=enabled", context="autopilot resume output")
        autopilot_status = run_command([str(binary_path), "autopilot", "status"], env=env, cwd=repo_root)
        assert_in(autopilot_status.stdout, "autopilot=enabled", context="autopilot status output")

        evolve_status = run_command([str(binary_path), "evolve", "status"], env=env, cwd=repo_root)
        assert_in(evolve_status.stdout, "state=", context="evolve status output")

        logs = run_command([str(binary_path), "logs", "--limit", "20"], env=env, cwd=repo_root)
        assert_true(logs.stdout.strip() != "", message="logs output should not be empty after operator actions")

        log_step("restarting daemon to verify persistence")
        run_command([str(binary_path), "daemon", "stop"], env=env, cwd=repo_root)
        wait_for_daemon_down(status_url, headers=auth_headers)
        run_command(
            [str(binary_path), "daemon", "start"],
            env=env,
            cwd=repo_root,
            capture_output=False,
        )
        wait_for_http_json(status_url, headers=auth_headers)

        persisted_plugins = json.loads(
            run_command([str(binary_path), "plugin", "list", "--json"], env=env, cwd=repo_root).stdout
        )
        assert_true(any(entry["id"] == PLUGIN_ID and entry["enabled"] and entry["trusted"] for entry in persisted_plugins), message="plugin state did not persist after restart")

        persisted_inboxes = run_command([str(binary_path), "inbox", "list"], env=env, cwd=repo_root)
        assert_in(persisted_inboxes.stdout, INBOX_ID, context="inbox list after restart")

        persisted_mcp = json.loads(
            run_command([str(binary_path), "mcp", "list", "--json"], env=env, cwd=repo_root).stdout
        )
        persisted_app = json.loads(
            run_command([str(binary_path), "app", "list", "--json"], env=env, cwd=repo_root).stdout
        )
        assert_true(any(entry["id"] == MCP_ID and entry["enabled"] for entry in persisted_mcp), message="MCP config did not persist after restart")
        assert_true(any(entry["id"] == APP_ID and entry["enabled"] for entry in persisted_app), message="App config did not persist after restart")

        persisted_memory = run_command(
            [str(binary_path), "memory", "search", MEMORY_SUBJECT],
            env=env,
            cwd=repo_root,
        )
        assert_in(persisted_memory.stdout, MEMORY_SUBJECT, context="memory search after restart")

        persisted_autopilot = run_command([str(binary_path), "autopilot", "status"], env=env, cwd=repo_root)
        assert_in(persisted_autopilot.stdout, "autopilot=enabled", context="autopilot status after restart")
        assert_in(persisted_autopilot.stdout, "interval=45s", context="autopilot status after restart")

        persisted_missions = run_command([str(binary_path), "mission", "list"], env=env, cwd=repo_root)
        assert_in(persisted_missions.stdout, mission_id, context="mission list after restart")

        log_step("cleaning up configured operator fixtures")
        run_command([str(binary_path), "memory", "forget", memory_id], env=env, cwd=repo_root)
        run_command([str(binary_path), "app", "remove", APP_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "mcp", "remove", MCP_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "inbox", "remove", INBOX_ID], env=env, cwd=repo_root)
        run_command([str(binary_path), "plugin", "remove", PLUGIN_ID], env=env, cwd=repo_root)

        print("Phase 2 smoke passed.")
        print(f"config_path={config_path}")
        print(f"data_path={doctor_values['data_path']}")
        return 0
    finally:
        try:
            log_step("cleanup: ensuring daemon is stopped")
            run_command([str(binary_path), "daemon", "stop"], env=env, cwd=repo_root, check=False)
        except Exception:
            pass
        log_step("cleanup: stopping mock provider")
        server.stop()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
