#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http import HTTPStatus
from http.cookiejar import CookieJar
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


PROMPT_TEXT = "Phase 1 smoke prompt"
MOCK_MODEL = "phase1-model"
MOCK_PROVIDER_ID = "mock-local"
MOCK_PROVIDER_NAME = "Mock Local"
DAEMON_TOKEN = "phase1-smoke-token"
DEFAULT_COMMAND_TIMEOUT = 120.0
LOG_PATH: Path | None = None


class MockProviderHandler(BaseHTTPRequestHandler):
    server_version = "Phase1MockProvider/1.0"

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
                {"data": [{"id": MOCK_MODEL}]},
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
        model = payload.get("model") or MOCK_MODEL
        self._send_json(
            HTTPStatus.OK,
            {
                "id": "chatcmpl-phase1",
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
    parser = argparse.ArgumentParser(description="Phase 1 isolated runtime smoke test")
    parser.add_argument("--binary-path", required=True)
    parser.add_argument("--repo-root", required=True)
    parser.add_argument("--scenario-root", required=True)
    parser.add_argument("--daemon-port", type=int, default=42891)
    parser.add_argument("--provider-port", type=int, default=42892)
    return parser.parse_args()


def with_suffix(text: str, *, suffix: str) -> str:
    return f"{text}\n{suffix}" if not text.endswith("\n") else f"{text}{suffix}"


def log_step(message: str) -> None:
    line = f"[phase1] {message}"
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
    opener: urllib.request.OpenerDirector | None = None,
) -> Any:
    data = None
    request_headers = dict(headers or {})
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        request_headers.setdefault("Content-Type", "application/json")
    request = urllib.request.Request(url, data=data, headers=request_headers, method=method)
    open_fn = opener.open if opener is not None else urllib.request.urlopen
    with open_fn(request, timeout=30) as response:
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


def update_config(config_path: Path, repo_root: Path, daemon_port: int, provider_port: int) -> dict[str, Any]:
    config = json.loads(config_path.read_text(encoding="utf-8"))
    config["daemon"]["host"] = "127.0.0.1"
    config["daemon"]["port"] = daemon_port
    config["daemon"]["token"] = DAEMON_TOKEN
    config["daemon"]["auto_start"] = False
    config["main_agent_alias"] = "main"
    config["providers"] = [
        {
            "id": MOCK_PROVIDER_ID,
            "display_name": MOCK_PROVIDER_NAME,
            "kind": "open_ai_compatible",
            "base_url": f"http://127.0.0.1:{provider_port}/v1",
            "auth_mode": "none",
            "default_model": MOCK_MODEL,
            "keychain_account": None,
            "oauth": None,
            "local": True,
        }
    ]
    config["aliases"] = [
        {
            "alias": "main",
            "provider_id": MOCK_PROVIDER_ID,
            "model": MOCK_MODEL,
            "description": "Phase 1 smoke main alias",
        }
    ]
    config["trust_policy"]["trusted_paths"] = [str(repo_root)]
    config["permission_preset"] = "auto_edit"
    config["onboarding_complete"] = True
    config_path.write_text(json.dumps(config, indent=2), encoding="utf-8")
    return config


def build_dashboard_opener() -> tuple[urllib.request.OpenerDirector, CookieJar]:
    cookie_jar = CookieJar()
    opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookie_jar))
    return opener, cookie_jar


def main() -> int:
    args = parse_args()
    binary_path = Path(args.binary_path).resolve()
    repo_root = Path(args.repo_root).resolve()
    scenario_root = Path(args.scenario_root).resolve()
    scenario_root.mkdir(parents=True, exist_ok=True)
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    scenario_root.mkdir(parents=True, exist_ok=True)
    global LOG_PATH
    LOG_PATH = scenario_root / "phase1-smoke.log"
    LOG_PATH.write_text("", encoding="utf-8")

    env = configure_profile_env(scenario_root)
    base_url = f"http://127.0.0.1:{args.daemon_port}"
    status_url = f"{base_url}/v1/status"
    auth_headers = {"Authorization": f"Bearer {DAEMON_TOKEN}"}
    data_path = ""

    server = MockProviderServer("127.0.0.1", args.provider_port)
    log_step(f"starting mock provider on 127.0.0.1:{args.provider_port}")
    server.start()
    try:
        wait_for_http_json(f"http://127.0.0.1:{args.provider_port}/v1/models")

        log_step("bootstrapping isolated profile with doctor")
        initial_doctor = run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        doctor_values = parse_key_value_output(initial_doctor.stdout)
        config_path = Path(doctor_values["config_path"])
        data_path = doctor_values["data_path"]
        updated_config = update_config(config_path, repo_root, args.daemon_port, args.provider_port)

        log_step("verifying doctor before daemon start")
        doctor_ready = run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        assert_in(doctor_ready.stdout, "daemon_running=false", context="doctor output before daemon start")
        assert_in(
            doctor_ready.stdout,
            f"{MOCK_PROVIDER_ID} ok=true",
            context="doctor output before daemon start",
        )

        log_step("starting daemon")
        run_command(
            [str(binary_path), "daemon", "start"],
            env=env,
            cwd=repo_root,
            capture_output=False,
        )
        wait_for_http_json(status_url, headers=auth_headers)

        log_step("checking daemon status")
        daemon_status = run_command([str(binary_path), "daemon", "status"], env=env, cwd=repo_root)
        assert_in(daemon_status.stdout, "running: true", context="daemon status after start")

        log_step("verifying dashboard launch flow")
        dashboard_urls = run_command(
            [str(binary_path), "dashboard", "--print-url", "--no-open"],
            env=env,
            cwd=repo_root,
        )
        assert_in(dashboard_urls.stdout, "Reusable dashboard URL:", context="dashboard command output")
        assert_in(
            dashboard_urls.stdout,
            "Immediate one-time connect URL",
            context="dashboard command output",
        )

        launch = request_json("POST", f"{base_url}/v1/dashboard/launch", headers=auth_headers, body={})
        opener, _cookie_jar = build_dashboard_opener()
        with opener.open(f"{base_url}{launch['launch_path']}", timeout=30) as response:
            response.read()
        bootstrap = request_json("GET", f"{base_url}/v1/dashboard/bootstrap", opener=opener)
        if bootstrap["status"]["main_agent_alias"] != "main":
            raise RuntimeError("dashboard bootstrap did not authenticate through cookie launch flow")

        log_step("running prompt execution")
        run_result = run_command(
            [str(binary_path), "exec", "--json", "--mode", "build", PROMPT_TEXT],
            env=env,
            cwd=repo_root,
        )
        run_event = json.loads(run_result.stdout.strip().splitlines()[-1])
        if run_event.get("event") != "response":
            raise RuntimeError(f"unexpected run event: {run_event}")
        if PROMPT_TEXT not in run_event["response"]:
            raise RuntimeError("mock provider response did not include the prompt text")
        session_id = run_event["session_id"]

        log_step("verifying persisted transcript and resume packet")
        transcript = request_json("GET", f"{base_url}/v1/sessions/{session_id}", headers=auth_headers)
        if transcript["session"]["task_mode"] != "build":
            raise RuntimeError("session task_mode was not persisted as build")
        if len(transcript["messages"]) < 2:
            raise RuntimeError("session transcript was not persisted")

        packet = request_json(
            "GET",
            f"{base_url}/v1/sessions/{session_id}/resume-packet",
            headers=auth_headers,
        )
        if packet["session"]["id"] != session_id or not packet["recent_messages"]:
            raise RuntimeError("resume packet did not include recent session context")

        log_step("verifying fork flow")
        forked = request_json(
            "POST",
            f"{base_url}/v1/sessions/{session_id}/fork",
            headers=auth_headers,
            body={},
        )
        if forked["session"]["id"] == session_id:
            raise RuntimeError("fork session returned the original session id")
        if forked["session"]["task_mode"] != "build":
            raise RuntimeError("forked session did not preserve task_mode")

        log_step("verifying compact flow")
        compacted = request_json(
            "POST",
            f"{base_url}/v1/sessions/{session_id}/compact",
            headers=auth_headers,
            body={},
        )
        if compacted["session"]["id"] in {session_id, forked["session"]["id"]}:
            raise RuntimeError("compact session did not create a fresh session")
        if not compacted["messages"]:
            raise RuntimeError("compacted session returned no transcript messages")
        compact_seed = compacted["messages"][0]["content"]
        if "compacted continuation" not in compact_seed:
            raise RuntimeError("compacted session did not seed the continuation summary")

        log_step("stopping daemon")
        run_command([str(binary_path), "daemon", "stop"], env=env, cwd=repo_root)
        wait_for_daemon_down(status_url, headers=auth_headers)
        daemon_stopped = run_command([str(binary_path), "daemon", "status"], env=env, cwd=repo_root)
        assert_in(daemon_stopped.stdout, "running: false", context="daemon status after stop")

        log_step("restarting daemon and verifying session persistence")
        run_command(
            [str(binary_path), "daemon", "start"],
            env=env,
            cwd=repo_root,
            capture_output=False,
        )
        wait_for_http_json(status_url, headers=auth_headers)
        persisted = request_json("GET", f"{base_url}/v1/sessions/{session_id}", headers=auth_headers)
        if persisted["session"]["id"] != session_id:
            raise RuntimeError("session did not survive daemon restart")

        log_step("checking doctor after restart")
        doctor_running = run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        assert_in(doctor_running.stdout, "daemon_running=true", context="doctor output after restart")
        assert_in(
            doctor_running.stdout,
            f"{MOCK_PROVIDER_ID} ok=true",
            context="doctor output after restart",
        )

        log_step("running reset flow")
        run_command([str(binary_path), "reset", "--yes"], env=env, cwd=repo_root)
        reset_doctor = run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        assert_in(reset_doctor.stdout, "daemon_running=false", context="doctor output after reset")
        reset_config = json.loads(config_path.read_text(encoding="utf-8"))
        if reset_config["onboarding_complete"]:
            raise RuntimeError("reset did not clear onboarding state")
        if reset_config["providers"]:
            raise RuntimeError("reset did not clear configured providers")

        log_step("verifying onboarding is required after reset")
        post_reset_exec = run_command(
            [str(binary_path), "exec", "post reset check"],
            env=env,
            cwd=repo_root,
            check=False,
        )
        if post_reset_exec.returncode == 0:
            raise RuntimeError("exec succeeded after reset without onboarding")
        combined = f"{post_reset_exec.stdout}\n{post_reset_exec.stderr}"
        if "no completed setup found" not in combined:
            raise RuntimeError("post-reset exec did not report onboarding was required")

        print("Phase 1 smoke passed.")
        print(f"config_path={config_path}")
        print(f"data_path={data_path}")
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
