from __future__ import annotations

import json
import os
import shlex
import shutil
import subprocess
import time
import urllib.request
from pathlib import Path
from typing import Any

from .common import (
    parse_key_value_output,
    python_launcher,
    read_json,
    run_command,
    write_json_config_raw,
)


DEFAULT_ALIAS = "main"
DEFAULT_PROVIDER_ID = "harness-local"
DEFAULT_PROVIDER_NAME = "Harness Local"


def _prepend_path_entry(env: dict[str, str], entry: Path) -> None:
    separator = ";" if os.name == "nt" else ":"
    current = env.get("PATH", "")
    entry_value = str(entry)
    env["PATH"] = f"{entry_value}{separator}{current}" if current else entry_value


def _ensure_python_shim(env: dict[str, str], scenario_root: Path) -> None:
    shim_dir = scenario_root / ("Scripts" if os.name == "nt" else "bin")
    shim_dir.mkdir(parents=True, exist_ok=True)
    launcher = python_launcher()
    if os.name == "nt":
        shim_path = shim_dir / "python.cmd"
        command = subprocess.list2cmdline(launcher)
        shim_path.write_text(f"@echo off\r\n{command} %*\r\n", encoding="utf-8")
    else:
        shim_path = shim_dir / "python"
        command = " ".join(shlex.quote(value) for value in launcher)
        shim_path.write_text(f"#!/usr/bin/env bash\nexec {command} \"$@\"\n", encoding="utf-8")
        shim_path.chmod(0o755)
    _prepend_path_entry(env, shim_dir)


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
    _ensure_python_shim(env, scenario_root)
    return env


def doctor_paths(binary_path: Path, repo_root: Path, env: dict[str, str]) -> dict[str, Path]:
    doctor = run_command([str(binary_path), "doctor"], cwd=repo_root, env=env, check=True)
    values = parse_key_value_output(doctor.stdout)
    return {
        "config_path": Path(values["config_path"]),
        "data_path": Path(values["data_path"]),
        "state_path": Path(values.get("state_path", values["data_path"])),
        "logs_path": Path(values.get("logs_path", values["data_path"])),
    }


def _normalise_trust_paths(paths: list[str] | None, extra_paths: list[Path]) -> list[str]:
    existing = [str(Path(value).resolve()) for value in (paths or [])]
    combined = set(existing)
    combined.update(str(path.resolve()) for path in extra_paths)
    return sorted(combined)


def _apply_common_config_updates(
    config: dict[str, Any],
    *,
    daemon_port: int,
    daemon_token: str,
    trust_paths: list[Path],
) -> dict[str, Any]:
    daemon = dict(config.get("daemon") or {})
    daemon["host"] = "127.0.0.1"
    daemon["port"] = daemon_port
    daemon["token"] = daemon_token
    daemon["auto_start"] = False
    config["daemon"] = daemon
    config["permission_preset"] = "full_auto"
    config["onboarding_complete"] = True
    trust_policy = dict(config.get("trust_policy") or {})
    trust_policy["trusted_paths"] = _normalise_trust_paths(trust_policy.get("trusted_paths"), trust_paths)
    trust_policy["allow_shell"] = True
    trust_policy["allow_network"] = True
    trust_policy["allow_self_edit"] = True
    trust_policy.setdefault("allow_full_disk", False)
    config["trust_policy"] = trust_policy
    return config


def bootstrap_mock_profile(
    repo_root: Path,
    binary_path: Path,
    scenario_root: Path,
    *,
    daemon_port: int,
    daemon_token: str,
    provider_base_url: str,
    provider_model: str,
    trust_paths: list[Path],
    provider_id: str = DEFAULT_PROVIDER_ID,
    provider_name: str = DEFAULT_PROVIDER_NAME,
    alias: str = DEFAULT_ALIAS,
) -> tuple[dict[str, str], dict[str, Any]]:
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    scenario_root.mkdir(parents=True, exist_ok=True)
    env = configure_profile_env(scenario_root)
    paths = doctor_paths(binary_path, repo_root, env)
    config = read_json(paths["config_path"])
    config = _apply_common_config_updates(
        config,
        daemon_port=daemon_port,
        daemon_token=daemon_token,
        trust_paths=trust_paths,
    )
    config["main_agent_alias"] = alias
    config["providers"] = [
        {
            "id": provider_id,
            "display_name": provider_name,
            "kind": "open_ai_compatible",
            "base_url": provider_base_url,
            "auth_mode": "none",
            "default_model": provider_model,
            "keychain_account": None,
            "oauth": None,
            "local": True,
        }
    ]
    config["aliases"] = [
        {
            "alias": alias,
            "provider_id": provider_id,
            "model": provider_model,
            "description": "Harness deterministic alias",
        }
    ]
    write_json_config_raw(paths["config_path"], config)
    return env, {
        "config_path": str(paths["config_path"]),
        "data_path": str(paths["data_path"]),
        "scenario_root": str(scenario_root),
        "daemon_port": daemon_port,
        "provider_base_url": provider_base_url,
        "provider_id": provider_id,
        "model": provider_model,
        "alias": alias,
    }


def clone_current_profile(
    repo_root: Path,
    binary_path: Path,
    scenario_root: Path,
    *,
    daemon_port: int,
    daemon_token: str,
    trust_paths: list[Path],
) -> tuple[dict[str, str], dict[str, Any]]:
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    scenario_root.mkdir(parents=True, exist_ok=True)

    source_paths = doctor_paths(binary_path, repo_root, os.environ.copy())
    env = configure_profile_env(scenario_root)
    dest_paths = doctor_paths(binary_path, repo_root, env)

    shutil.copy2(source_paths["config_path"], dest_paths["config_path"])
    if dest_paths["data_path"].exists():
        shutil.rmtree(dest_paths["data_path"])
    if source_paths["data_path"].exists():
        shutil.copytree(source_paths["data_path"], dest_paths["data_path"], dirs_exist_ok=True)

    config = read_json(dest_paths["config_path"])
    config = _apply_common_config_updates(
        config,
        daemon_port=daemon_port,
        daemon_token=daemon_token,
        trust_paths=trust_paths,
    )
    write_json_config_raw(dest_paths["config_path"], config)
    return env, {
        "config_path": str(dest_paths["config_path"]),
        "data_path": str(dest_paths["data_path"]),
        "source_config_path": str(source_paths["config_path"]),
        "source_data_path": str(source_paths["data_path"]),
        "scenario_root": str(scenario_root),
        "daemon_port": daemon_port,
    }


def provision_reference_profile(
    repo_root: Path,
    binary_path: Path,
    scenario_root: Path,
    *,
    daemon_port: int,
    daemon_token: str,
    trust_paths: list[Path],
    alias: str,
    provider_id: str,
    model: str | None,
    provider_kind: str,
    base_url: str | None,
    api_key_env: str | None,
) -> tuple[dict[str, str], dict[str, Any]]:
    if scenario_root.exists():
        shutil.rmtree(scenario_root)
    scenario_root.mkdir(parents=True, exist_ok=True)
    env = configure_profile_env(scenario_root)
    paths = doctor_paths(binary_path, repo_root, env)

    config = read_json(paths["config_path"])
    config = _apply_common_config_updates(
        config,
        daemon_port=daemon_port,
        daemon_token=daemon_token,
        trust_paths=trust_paths,
    )
    write_json_config_raw(paths["config_path"], config)

    api_key = None
    if api_key_env:
        api_key = os.environ.get(api_key_env, "")
        if not api_key:
            raise SystemExit(f"Provider profile requested api_key_env={api_key_env}, but the variable is not set.")

    provider_kind_lower = provider_kind.strip().lower()
    is_local_openai = provider_kind_lower == "openai-compatible" and (base_url or "").startswith(
        ("http://127.0.0.1", "http://localhost", "https://127.0.0.1", "https://localhost")
    )
    is_local = provider_kind_lower in {"ollama"} or is_local_openai
    display_name = provider_id.replace("-", " ").title()
    argv = [str(binary_path), "provider", "add-local" if is_local else "add"]
    argv += ["--id", provider_id, "--name", display_name, "--kind", provider_kind_lower]
    if base_url:
        argv += ["--base-url", base_url]
    if model:
        argv += ["--model", model]
    if api_key:
        argv += ["--api-key", api_key]
    argv += ["--main-alias", alias]
    run_command(argv, cwd=repo_root, env=env, check=True)

    config = read_json(paths["config_path"])
    config = _apply_common_config_updates(
        config,
        daemon_port=daemon_port,
        daemon_token=daemon_token,
        trust_paths=trust_paths,
    )
    config["main_agent_alias"] = alias
    write_json_config_raw(paths["config_path"], config)
    return env, {
        "config_path": str(paths["config_path"]),
        "data_path": str(paths["data_path"]),
        "scenario_root": str(scenario_root),
        "daemon_port": daemon_port,
        "provider_id": provider_id,
        "provider_kind": provider_kind_lower,
        "alias": alias,
        "model": model,
        "base_url": base_url,
        "api_key_env": api_key_env,
    }


def request_json(url: str, *, headers: dict[str, str] | None = None) -> Any:
    request = urllib.request.Request(url, headers=dict(headers or {}), method="GET")
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read().decode("utf-8"))


def wait_for_http_json(url: str, *, headers: dict[str, str] | None = None, timeout_seconds: float = 30.0) -> Any:
    deadline = time.time() + timeout_seconds
    while True:
        try:
            return request_json(url, headers=headers)
        except Exception:
            if time.time() >= deadline:
                raise
            time.sleep(0.3)


def start_daemon(repo_root: Path, binary_path: Path, env: dict[str, str]) -> None:
    run_command([str(binary_path), "daemon", "start"], cwd=repo_root, env=env, capture_output=False, check=True)


def stop_daemon(repo_root: Path, binary_path: Path, env: dict[str, str]) -> None:
    run_command([str(binary_path), "daemon", "stop"], cwd=repo_root, env=env, capture_output=False, check=False)
