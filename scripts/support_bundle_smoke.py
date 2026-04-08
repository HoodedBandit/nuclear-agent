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
import urllib.request
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


DAEMON_TOKEN = "support-bundle-smoke-token"
MOCK_PROVIDER_ID = "support-bundle-local"
MOCK_PROVIDER_NAME = "Support Bundle Local"
MOCK_MODEL = "support-bundle-model"
PROMPT_TEXT = "Support bundle smoke prompt"
DEFAULT_COMMAND_TIMEOUT = 120.0


class MockProviderHandler(BaseHTTPRequestHandler):
    server_version = "SupportBundleMockProvider/1.0"

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
            self._send_json(HTTPStatus.OK, {"data": [{"id": MOCK_MODEL}]})
            return
        self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        if self.path != "/v1/chat/completions":
            self._send_json(HTTPStatus.NOT_FOUND, {"error": "not found"})
            return

        length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(length) or b"{}")
        model = payload.get("model") or MOCK_MODEL
        prompt = PROMPT_TEXT
        for message in reversed(payload.get("messages") or []):
            if message.get("role") != "user":
                continue
            content = message.get("content")
            if isinstance(content, str):
                prompt = content
                break
        self._send_json(
            HTTPStatus.OK,
            {
                "id": "chatcmpl-support-bundle",
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
    parser = argparse.ArgumentParser(description="Support bundle smoke test")
    parser.add_argument("--binary-path", required=True)
    parser.add_argument("--repo-root", required=True)
    parser.add_argument("--scenario-root", required=True)
    parser.add_argument("--daemon-port", type=int, default=0)
    parser.add_argument("--provider-port", type=int, default=0)
    return parser.parse_args()


def run_command(
    args: list[str],
    *,
    env: dict[str, str],
    cwd: Path,
    check: bool = True,
    timeout: float = DEFAULT_COMMAND_TIMEOUT,
    capture_output: bool = True,
) -> subprocess.CompletedProcess[str]:
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
    if check and result.returncode != 0:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(args)}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
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


def allocate_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as handle:
        handle.bind(("127.0.0.1", 0))
        handle.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        return int(handle.getsockname()[1])


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


def update_config(config_path: Path, repo_root: Path, daemon_port: int, provider_port: int) -> None:
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
            "description": "Support bundle smoke alias",
        }
    ]
    config["trust_policy"]["trusted_paths"] = [str(repo_root)]
    config["permission_preset"] = "auto_edit"
    config["onboarding_complete"] = True
    config_path.write_text(json.dumps(config, indent=2), encoding="utf-8")


def assert_file(path: Path) -> None:
    if not path.exists():
        raise RuntimeError(f"expected file at {path}")


def assert_not_contains(value: str, needle: str, *, context: str) -> None:
    if needle in value:
        raise RuntimeError(f"unexpected {needle!r} in {context}")


def main() -> int:
    args = parse_args()
    binary_path = Path(args.binary_path).resolve()
    repo_root = Path(args.repo_root).resolve()
    scenario_root = Path(args.scenario_root).resolve()
    daemon_port = args.daemon_port or allocate_local_port()
    provider_port = args.provider_port or allocate_local_port()
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    scenario_root.mkdir(parents=True, exist_ok=True)

    env = configure_profile_env(scenario_root)
    base_url = f"http://127.0.0.1:{daemon_port}"
    auth_headers = {"Authorization": f"Bearer {DAEMON_TOKEN}"}
    bundle_dir = scenario_root / "support-bundle"

    server = MockProviderServer("127.0.0.1", provider_port)
    server.start()
    try:
        wait_for_http_json(f"http://127.0.0.1:{provider_port}/v1/models")

        doctor = run_command([str(binary_path), "doctor"], env=env, cwd=repo_root)
        doctor_values = parse_key_value_output(doctor.stdout)
        config_path = Path(doctor_values["config_path"])
        update_config(config_path, repo_root, daemon_port, provider_port)

        run_command(
            [str(binary_path), "daemon", "start"],
            env=env,
            cwd=repo_root,
            capture_output=False,
        )
        wait_for_http_json(f"{base_url}/v1/status", headers=auth_headers)

        run_result = run_command(
            [str(binary_path), "exec", "--json", "--mode", "build", PROMPT_TEXT],
            env=env,
            cwd=repo_root,
        )
        event = json.loads(run_result.stdout.strip().splitlines()[-1])
        if event.get("event") != "response":
            raise RuntimeError(f"unexpected exec event: {event}")

        support_bundle = run_command(
            [
                str(binary_path),
                "support-bundle",
                "--output-dir",
                str(bundle_dir),
                "--log-limit",
                "25",
                "--session-limit",
                "10",
            ],
            env=env,
            cwd=repo_root,
        )
        if f"support_bundle={bundle_dir}" not in support_bundle.stdout:
            raise RuntimeError(f"unexpected support bundle output:\n{support_bundle.stdout}")

        manifest_path = bundle_dir / "manifest.json"
        doctor_path = bundle_dir / "doctor.json"
        config_summary_path = bundle_dir / "config-summary.json"
        sessions_path = bundle_dir / "sessions.json"
        logs_path = bundle_dir / "logs.json"
        readme_path = bundle_dir / "README.md"
        daemon_status_path = bundle_dir / "daemon-status.json"

        for path in [
            manifest_path,
            doctor_path,
            config_summary_path,
            sessions_path,
            logs_path,
            readme_path,
            daemon_status_path,
        ]:
            assert_file(path)

        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        config_summary = json.loads(config_summary_path.read_text(encoding="utf-8"))
        sessions = json.loads(sessions_path.read_text(encoding="utf-8"))
        logs = json.loads(logs_path.read_text(encoding="utf-8"))
        readme = readme_path.read_text(encoding="utf-8")

        if manifest.get("daemon_status_file") != "daemon-status.json":
            raise RuntimeError(f"manifest did not record daemon status: {manifest}")
        if config_summary.get("main_agent_alias") != "main":
            raise RuntimeError("support bundle did not capture the configured main alias")
        if config_summary.get("providers", [{}])[0].get("id") != MOCK_PROVIDER_ID:
            raise RuntimeError("support bundle did not capture the configured provider")
        if not sessions:
            raise RuntimeError("support bundle did not capture sessions")
        if not logs:
            raise RuntimeError("support bundle did not capture logs")

        serialized_summary = json.dumps(config_summary)
        assert_not_contains(serialized_summary, DAEMON_TOKEN, context="config summary")
        assert_not_contains(serialized_summary, "keychain_account", context="config summary")
        assert_not_contains(readme, DAEMON_TOKEN, context="support bundle README")

        print("Support bundle smoke passed.")
        print(f"bundle_dir={bundle_dir}")
        return 0
    finally:
        try:
            run_command([str(binary_path), "daemon", "stop"], env=env, cwd=repo_root, check=False)
        except Exception:
            pass
        server.stop()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
