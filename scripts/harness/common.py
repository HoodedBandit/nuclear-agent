from __future__ import annotations

import json
import os
import re
import shutil
import socket
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REDACTED = "[REDACTED]"
SENSITIVE_KEYS = (
    "access_token",
    "refresh_token",
    "id_token",
    "api_key",
    "authorization",
    "password",
    "secret",
    "subject_token",
    "daemon_token",
    "token",
)
KEY_VALUE_PATTERN = re.compile(
    r"(?i)\b("
    + "|".join(re.escape(key) for key in SENSITIVE_KEYS)
    + r")\b\s*[:=]\s*([^\s,\"';]+)"
)
FLAG_VALUE_PATTERN = re.compile(
    r"(?i)(--?(?:"
    + "|".join(re.escape(key).replace("_", "[-_]") for key in SENSITIVE_KEYS)
    + r"))\s+([^\s,\"';]+)"
)
BEARER_PATTERN = re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._-]{6,}")
TOKEN_PREFIX_PATTERN = re.compile(
    r"\b(?:sk-[A-Za-z0-9_-]+|gh[pousr]_[A-Za-z0-9_]+|glpat-[A-Za-z0-9_-]+|xox[baprs]-[A-Za-z0-9-]+)\b"
)
JWT_PATTERN = re.compile(r"\b[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\b")


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


def write_json_config_raw(path: Path, payload: Any) -> None:
    """Write local harness config fixtures that must preserve test credentials."""
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def sanitize_text(value: str) -> str:
    redacted = BEARER_PATTERN.sub(f"Bearer {REDACTED}", value)
    redacted = KEY_VALUE_PATTERN.sub(lambda match: f"{match.group(1)}={REDACTED}", redacted)
    redacted = FLAG_VALUE_PATTERN.sub(lambda match: f"{match.group(1)} {REDACTED}", redacted)
    redacted = TOKEN_PREFIX_PATTERN.sub(REDACTED, redacted)
    redacted = JWT_PATTERN.sub(REDACTED, redacted)
    return redacted


def sanitize_artifact_payload(payload: Any) -> Any:
    if isinstance(payload, dict):
        sanitized: dict[Any, Any] = {}
        for key, value in payload.items():
            key_text = str(key).strip().lower()
            if any(fragment in key_text for fragment in SENSITIVE_KEYS):
                sanitized[key] = REDACTED
            else:
                sanitized[key] = sanitize_artifact_payload(value)
        return sanitized
    if isinstance(payload, list):
        return [sanitize_artifact_payload(value) for value in payload]
    if isinstance(payload, str):
        return sanitize_text(payload)
    return payload


def write_json_artifact(path: Path, payload: Any) -> None:
    sanitized = sanitize_artifact_payload(payload)
    serialized = json.dumps(sanitized, indent=2)
    path.write_bytes(sanitize_text(serialized).encode("utf-8"))


def write_text_artifact(path: Path, content: str) -> None:
    path.write_text(sanitize_text(content), encoding="utf-8")


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
            f"Command failed ({completed.returncode}): {sanitize_text(command_label(argv))}\n"
            f"stdout:\n{sanitize_text(completed.stdout)}\n"
            f"stderr:\n{sanitize_text(completed.stderr)}"
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
