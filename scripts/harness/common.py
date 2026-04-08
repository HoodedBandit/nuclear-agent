from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def repo_root_from_script(script_path: Path) -> Path:
    return script_path.resolve().parent.parent


def is_windows() -> bool:
    return os.name == "nt"


def executable_suffix() -> str:
    return ".exe" if is_windows() else ""


def resolve_existing_path(repo_root: Path, value: str) -> Path:
    candidate = Path(value).expanduser()
    if not candidate.is_absolute():
        candidate = (repo_root / candidate).resolve()
    return candidate


def ensure_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    return path


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def resolve_binary_path(repo_root: Path, provided: str | None) -> Path:
    if provided:
        candidate = resolve_existing_path(repo_root, provided)
        if candidate.exists():
            return candidate
        raise SystemExit(f"Binary not found: {candidate}")

    binary_name = f"nuclear{executable_suffix()}"
    candidates = [
        repo_root / "target" / "verify-workspace" / "release" / binary_name,
        repo_root / "target" / "release" / binary_name,
        repo_root / "target" / "debug" / binary_name,
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate.resolve()
    raise SystemExit(
        "Could not find a built nuclear binary under target/{verify-workspace,release,debug}. "
        "Build the workspace first or pass --binary-path."
    )


def allocate_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as handle:
        handle.bind(("127.0.0.1", 0))
        handle.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        return int(handle.getsockname()[1])


def command_label(argv: list[str]) -> str:
    return subprocess.list2cmdline(argv) if is_windows() else " ".join(argv)


def resolve_command_argv(argv: list[str]) -> list[str]:
    if not argv:
        return []

    if argv[0] != "python":
        return list(argv)

    if shutil.which("python"):
        return list(argv)

    if is_windows() and shutil.which("py"):
        return ["py", "-3", *argv[1:]]

    if shutil.which("python3"):
        return ["python3", *argv[1:]]

    return list(argv)


def run_command(
    argv: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    timeout_seconds: float | None = None,
    capture_output: bool = True,
    check: bool = False,
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        argv,
        cwd=str(cwd),
        env=env,
        capture_output=capture_output,
        check=False,
        text=True,
        timeout=timeout_seconds,
        encoding="utf-8",
        errors="replace",
    )
    if check and completed.returncode != 0:
        raise RuntimeError(
            f"Command failed ({completed.returncode}): {command_label(argv)}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return completed


def parse_key_value_output(output: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw_line in output.splitlines():
        line = raw_line.strip()
        if not line or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def parse_json_output(stdout_text: str) -> tuple[list[dict[str, Any]], dict[str, Any] | None]:
    events: list[dict[str, Any]] = []
    final_response: dict[str, Any] | None = None
    for raw_line in stdout_text.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        try:
            parsed = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict):
            events.append(parsed)
            if parsed.get("event") == "response":
                final_response = parsed
    if not events:
        stripped = stdout_text.strip()
        if stripped:
            try:
                parsed = json.loads(stripped)
            except json.JSONDecodeError:
                return [], None
            if isinstance(parsed, dict):
                events = [parsed]
                if parsed.get("event") == "response":
                    final_response = parsed
    return events, final_response


def python_launcher() -> list[str]:
    if sys.executable:
        return [sys.executable]
    return ["python"]
