from __future__ import annotations

import fnmatch
import shutil
import subprocess
from pathlib import Path
from typing import Any

from . import bootstrap
from .common import (
    allocate_local_port,
    command_label,
    ensure_dir,
    parse_json_output,
    resolve_command_argv,
    run_command,
    utc_now_iso,
    write_json_artifact,
    write_text_artifact,
)
from .provider_adapters import ANALYSIS_MODEL, SCRIPTED_MODEL, HarnessProviderServer


IGNORED_CHANGE_GLOBS = [".git/**", "__pycache__/**", ".pytest_cache/**", "*.pyc", "*.pyo"]


def _summary_markdown(title: str, results: list[dict[str, Any]], *, lane: str, started_at: str, finished_at: str) -> str:
    lines = [
        f"# {title}",
        "",
        f"- lane: `{lane}`",
        f"- started_at: `{started_at}`",
        f"- finished_at: `{finished_at}`",
        f"- task_count: `{len(results)}`",
        f"- passed: `{sum(1 for result in results if result.get('passed'))}`",
        f"- failed: `{sum(1 for result in results if not result.get('passed'))}`",
        "",
        "| Item | Passed | Notes |",
        "| --- | --- | --- |",
    ]
    for result in results:
        note = str(result.get("summary") or result.get("error") or "")
        lines.append(f"| {result['id']} | {'yes' if result.get('passed') else 'no'} | {note} |")
    return "\n".join(lines) + "\n"


def _task_cwd(repo_root: Path, cwd_value: str) -> Path:
    candidate = Path(cwd_value).expanduser()
    if not candidate.is_absolute():
        candidate = (repo_root / candidate).resolve()
    return candidate


def run_analysis_tasks(
    *,
    repo_root: Path,
    binary_path: Path,
    task_file: Path,
    tasks: list[dict[str, Any]],
    run_dir: Path,
) -> dict[str, Any]:
    output_root = ensure_dir(run_dir / "analysis-smoke")
    scenario_root = output_root / "scenario"
    provider_port = allocate_local_port()
    daemon_port = allocate_local_port()
    daemon_token = "analysis-harness-token"
    server = HarnessProviderServer(
        host="127.0.0.1",
        port=provider_port,
        mode="analysis",
        default_model=ANALYSIS_MODEL,
    )
    server.start()
    env, bootstrap_info = bootstrap.bootstrap_mock_profile(
        repo_root,
        binary_path,
        scenario_root,
        daemon_port=daemon_port,
        daemon_token=daemon_token,
        provider_base_url=f"http://127.0.0.1:{provider_port}/v1",
        provider_model=ANALYSIS_MODEL,
        trust_paths=[repo_root],
    )
    bootstrap.start_daemon(repo_root, binary_path, env)
    bootstrap.wait_for_http_json(
        f"http://127.0.0.1:{daemon_port}/v1/status",
        headers={"Authorization": f"Bearer {daemon_token}"},
    )

    started_at = utc_now_iso()
    results: list[dict[str, Any]] = []
    try:
        for task in tasks:
            task_dir = ensure_dir(output_root / task["id"])
            write_json_artifact(task_dir / "request.json", task)
            stdout_path = task_dir / "stdout.txt"
            stderr_path = task_dir / "stderr.txt"
            with stdout_path.open("w", encoding="utf-8") as stdout_handle, stderr_path.open(
                "w", encoding="utf-8"
            ) as stderr_handle:
                completed = subprocess.run(
                    [str(binary_path), *task["command"]],
                    cwd=str(_task_cwd(repo_root, task.get("cwd") or ".")),
                    env=env,
                    stdout=stdout_handle,
                    stderr=stderr_handle,
                    text=True,
                    check=False,
                    encoding="utf-8",
                    errors="replace",
                )
            stdout_text = stdout_path.read_text(encoding="utf-8")
            stderr_text = stderr_path.read_text(encoding="utf-8")
            events, final_response = parse_json_output(stdout_text)
            result = {
                "id": task["id"],
                "description": task.get("description") or "",
                "category": task.get("category") or "",
                "exit_code": completed.returncode,
                "expected_exit_code": int(task.get("expected_exit_code", 0)),
                "passed": completed.returncode == int(task.get("expected_exit_code", 0)),
                "summary": "passed" if completed.returncode == int(task.get("expected_exit_code", 0)) else "command failed",
                "artifacts": {
                    "request": str(task_dir / "request.json"),
                    "stdout": str(stdout_path),
                    "stderr": str(stderr_path),
                },
            }
            if events:
                events_path = task_dir / "events.json"
                write_json_artifact(events_path, events)
                result["artifacts"]["events"] = str(events_path)
            if final_response:
                for key in ("session_id", "alias", "provider_id", "model"):
                    if final_response.get(key):
                        result[key] = final_response[key]
                if isinstance(final_response.get("structured_output"), dict):
                    structured_path = task_dir / "structured_output.json"
                    write_json_artifact(structured_path, final_response["structured_output"])
                    result["artifacts"]["structured_output"] = str(structured_path)
            if stderr_text.strip():
                result["stderr_preview"] = stderr_text.strip().splitlines()[:3]
            write_json_artifact(task_dir / "result.json", result)
            results.append(result)
    finally:
        bootstrap.stop_daemon(repo_root, binary_path, env)
        server.stop()

    finished_at = utc_now_iso()
    summary = {
        "lane": "analysis-smoke",
        "task_file": str(task_file),
        "binary_path": str(binary_path),
        "run_dir": str(output_root),
        "started_at": started_at,
        "finished_at": finished_at,
        "task_count": len(results),
        "passed": sum(1 for result in results if result["passed"]),
        "failed": sum(1 for result in results if not result["passed"]),
        "bootstrap_profile": bootstrap_info,
        "results": results,
    }
    write_json_artifact(output_root / "summary.json", summary)
    (output_root / "summary.md").write_text(
        _summary_markdown("Analysis Smoke Summary", results, lane="analysis-smoke", started_at=started_at, finished_at=finished_at),
        encoding="utf-8",
    )
    return summary


def _run_command_spec(
    command: dict[str, Any],
    *,
    cwd: Path,
    env: dict[str, str] | None,
    artifact_root: Path,
    prefix: str,
) -> dict[str, Any]:
    command_root = ensure_dir(artifact_root / prefix)
    stdout_path = command_root / "stdout.txt"
    stderr_path = command_root / "stderr.txt"
    resolved_argv = resolve_command_argv(command["argv"])
    completed = run_command(
        resolved_argv,
        cwd=(cwd / command["workdir"]).resolve(),
        env=env,
        timeout_seconds=float(command["timeout_seconds"]),
        capture_output=True,
    )
    write_text_artifact(stdout_path, completed.stdout)
    write_text_artifact(stderr_path, completed.stderr)
    result = {
        "label": command["label"],
        "argv": resolved_argv,
        "command": command_label(resolved_argv),
        "cwd": str((cwd / command["workdir"]).resolve()),
        "expected_exit_code": int(command["expected_exit_code"]),
        "exit_code": completed.returncode,
        "passed": completed.returncode == int(command["expected_exit_code"]),
        "artifacts": {"stdout": str(stdout_path), "stderr": str(stderr_path)},
    }
    write_json_artifact(command_root / "result.json", result)
    return result


def _copy_fixture(src: Path, dest: Path) -> None:
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(src, dest)


def _init_fixture_repo(workspace: Path) -> None:
    run_command(["git", "init"], cwd=workspace, check=True)
    run_command(["git", "config", "user.name", "Nuclear Harness"], cwd=workspace, check=True)
    run_command(["git", "config", "user.email", "harness@nuclear.local"], cwd=workspace, check=True)
    run_command(["git", "add", "."], cwd=workspace, check=True)
    run_command(["git", "commit", "-m", "fixture"], cwd=workspace, check=True)


def _git_changed_paths(workspace: Path) -> list[str]:
    changed = run_command(["git", "diff", "--name-only", "HEAD"], cwd=workspace, check=True).stdout.splitlines()
    untracked = run_command(["git", "ls-files", "--others", "--exclude-standard"], cwd=workspace, check=True).stdout.splitlines()
    paths = {path.replace("\\", "/").strip() for path in [*changed, *untracked] if path.strip()}
    filtered = []
    for path in sorted(paths):
        if any(fnmatch.fnmatchcase(path, pattern) for pattern in IGNORED_CHANGE_GLOBS):
            continue
        filtered.append(path)
    return filtered


def _write_exec_artifacts(task_dir: Path, stdout_text: str, stderr_text: str) -> tuple[list[dict[str, Any]], dict[str, Any] | None]:
    events, final_response = parse_json_output(stdout_text)
    if events:
        write_json_artifact(task_dir / "events.json", events)
    if final_response:
        write_json_artifact(task_dir / "final_response.json", final_response)
    if stderr_text.strip():
        write_text_artifact(task_dir / "stderr-preview.txt", "\n".join(stderr_text.strip().splitlines()[:20]))
    return events, final_response


def _evaluate_final_response(assertions: dict[str, list[str]], response_text: str) -> list[str]:
    failures: list[str] = []
    for value in assertions.get("contains_all") or []:
        if value not in response_text:
            failures.append(f"final response is missing required text: {value!r}")
    contains_any = assertions.get("contains_any") or []
    if contains_any and not any(value in response_text for value in contains_any):
        failures.append("final response did not contain any accepted marker")
    for value in assertions.get("not_contains") or []:
        if value in response_text:
            failures.append(f"final response unexpectedly contained text: {value!r}")
    return failures


def _evaluate_changed_paths(task: dict[str, Any], changed_paths: list[str]) -> list[str]:
    failures: list[str] = []
    allowed = task.get("allowed_change_globs") or []
    forbidden = task.get("forbidden_change_globs") or []
    expected = task.get("expected_changed_paths") or []
    for path in changed_paths:
        if forbidden and any(fnmatch.fnmatchcase(path, pattern) for pattern in forbidden):
            failures.append(f"changed forbidden path: {path}")
        if allowed and not any(fnmatch.fnmatchcase(path, pattern) for pattern in allowed):
            failures.append(f"changed path outside allowed globs: {path}")
    for path in expected:
        if path not in changed_paths:
            failures.append(f"expected changed path was not modified: {path}")
    return failures


def _tool_names(events: list[dict[str, Any]]) -> list[str]:
    return [str(event.get("name")) for event in events if event.get("event") == "tool" and event.get("name")]


def _run_coding_task(
    *,
    repo_root: Path,
    binary_path: Path,
    task_file: Path,
    task: dict[str, Any],
    task_dir: Path,
    lane: str,
    reference_profile: dict[str, Any] | None,
    scripted_server_port: int | None,
) -> dict[str, Any]:
    fixture_root = (task_file.parent / str(task["fixture"])).resolve()
    workspace = task_dir / "workspace"
    scenario_root = task_dir / "scenario"
    checks_root = ensure_dir(task_dir / "checks")
    _copy_fixture(fixture_root, workspace)
    _init_fixture_repo(workspace)

    failures: list[str] = []
    for index, command in enumerate(task.get("setup_commands") or []):
        result = _run_command_spec(command, cwd=workspace, env=None, artifact_root=checks_root, prefix=f"setup-{index + 1}")
        if not result["passed"]:
            return {
                "id": task["id"],
                "passed": False,
                "summary": f"setup command failed: {command['label']}",
                "artifacts": {"checks": str(checks_root)},
                "failures": [f"setup command failed: {command['label']}"],
            }

    for index, command in enumerate(task.get("precondition_commands") or []):
        result = _run_command_spec(command, cwd=workspace, env=None, artifact_root=checks_root, prefix=f"precondition-{index + 1}")
        if not result["passed"]:
            return {
                "id": task["id"],
                "passed": False,
                "summary": f"precondition failed: {command['label']}",
                "artifacts": {"checks": str(checks_root)},
                "failures": [f"precondition failed: {command['label']}"],
            }

    daemon_port = allocate_local_port()
    daemon_token = f"harness-{lane}-token"
    provider_base_url = f"http://127.0.0.1:{scripted_server_port}/v1" if scripted_server_port else ""
    if lane == "coding-deterministic":
        env, profile_info = bootstrap.bootstrap_mock_profile(
            repo_root,
            binary_path,
            scenario_root,
            daemon_port=daemon_port,
            daemon_token=daemon_token,
            provider_base_url=provider_base_url,
            provider_model=SCRIPTED_MODEL,
            trust_paths=[workspace],
        )
        alias = "main"
    else:
        alias = str((reference_profile or {}).get("alias") or "main")
        if reference_profile and any(reference_profile.get(key) for key in ("provider_id", "provider_kind", "base_url", "api_key_env", "model")):
            provider_kind = str(reference_profile.get("provider_kind") or "")
            provider_id = str(reference_profile.get("provider_id") or "harness-reference")
            model = str(reference_profile.get("model") or "") or None
            base_url = str(reference_profile.get("base_url") or "") or None
            api_key_env = str(reference_profile.get("api_key_env") or "") or None
            if not provider_kind:
                raise SystemExit("coding-reference explicit profile requires provider_kind.")
            env, profile_info = bootstrap.provision_reference_profile(
                repo_root,
                binary_path,
                scenario_root,
                daemon_port=daemon_port,
                daemon_token=daemon_token,
                trust_paths=[workspace],
                alias=alias,
                provider_id=provider_id,
                model=model,
                provider_kind=provider_kind,
                base_url=base_url,
                api_key_env=api_key_env,
            )
        else:
            env, profile_info = bootstrap.clone_current_profile(
                repo_root,
                binary_path,
                scenario_root,
                daemon_port=daemon_port,
                daemon_token=daemon_token,
                trust_paths=[workspace],
            )

    exec_stdout_path = task_dir / "exec.stdout.txt"
    exec_stderr_path = task_dir / "exec.stderr.txt"
    success_results: list[dict[str, Any]] = []
    try:
        for index, command in enumerate(task.get("expected_failure_before_run") or []):
            result = _run_command_spec(
                command,
                cwd=workspace,
                env=None,
                artifact_root=checks_root,
                prefix=f"expected-failure-{index + 1}",
            )
            if not result["passed"]:
                failures.append(f"expected failing command did not match its expected exit code: {command['label']}")

        bootstrap.start_daemon(repo_root, binary_path, env)
        bootstrap.wait_for_http_json(
            f"http://127.0.0.1:{daemon_port}/v1/status",
            headers={"Authorization": f"Bearer {daemon_token}"},
        )

        prompt = f"[HARNESS_TASK_ID:{task['id']}] {task['prompt']}"
        completed = run_command(
            [
                str(binary_path),
                "exec",
                "--alias",
                alias,
                "--json",
                "--ephemeral",
                "--permissions",
                "full-auto",
                prompt,
            ],
            cwd=workspace,
            env=env,
            timeout_seconds=float(task["max_duration_seconds"]) + 30.0,
        )
        write_text_artifact(exec_stdout_path, completed.stdout)
        write_text_artifact(exec_stderr_path, completed.stderr)
        events, final_response = _write_exec_artifacts(task_dir, completed.stdout, completed.stderr)

        if completed.returncode != 0:
            failures.append(f"agent execution exited with code {completed.returncode}")
        tool_names = _tool_names(events)
        if len(tool_names) > int(task["max_tool_calls"]):
            failures.append(f"task exceeded max_tool_calls ({len(tool_names)} > {int(task['max_tool_calls'])})")
        for required_tool in task.get("required_tools") or []:
            if required_tool not in tool_names:
                failures.append(f"required tool was not used: {required_tool}")

        response_text = str((final_response or {}).get("response") or "")
        failures.extend(_evaluate_final_response(task["final_response_assertions"], response_text))

        for index, command in enumerate(task.get("success_commands") or []):
            result = _run_command_spec(command, cwd=workspace, env=None, artifact_root=checks_root, prefix=f"success-{index + 1}")
            success_results.append(result)
            if not result["passed"]:
                failures.append(f"success command failed: {command['label']}")

        for index, command in enumerate(task.get("post_run_assertions") or []):
            result = _run_command_spec(command, cwd=workspace, env=None, artifact_root=checks_root, prefix=f"post-run-{index + 1}")
            if not result["passed"]:
                failures.append(f"post-run assertion failed: {command['label']}")

        changed_paths = _git_changed_paths(workspace)
        failures.extend(_evaluate_changed_paths(task, changed_paths))
        write_json_artifact(task_dir / "changed-paths.json", changed_paths)
        diff = run_command(["git", "diff", "--stat", "--patch", "HEAD"], cwd=workspace, check=True).stdout
        (task_dir / "diff.patch").write_text(diff, encoding="utf-8")
    finally:
        bootstrap.stop_daemon(repo_root, binary_path, env)
        for index, command in enumerate(task.get("cleanup_commands") or []):
            _run_command_spec(command, cwd=workspace, env=None, artifact_root=checks_root, prefix=f"cleanup-{index + 1}")

    return {
        "id": task["id"],
        "suite": task["suite"],
        "prompt": task["prompt"],
        "passed": len(failures) == 0,
        "summary": "passed" if not failures else "; ".join(failures[:3]),
        "artifacts": {
            "workspace": str(workspace),
            "scenario_root": str(scenario_root),
            "checks": str(checks_root),
            "stdout": str(exec_stdout_path),
            "stderr": str(exec_stderr_path),
            "profile": profile_info,
        },
        "failures": failures,
        "success_checks": success_results,
    }


def run_coding_tasks(
    *,
    repo_root: Path,
    binary_path: Path,
    task_file: Path,
    tasks: list[dict[str, Any]],
    run_dir: Path,
    lane: str,
    reference_profile: dict[str, Any] | None = None,
    scripted_server: HarnessProviderServer | None = None,
) -> dict[str, Any]:
    output_root = ensure_dir(run_dir / lane)
    started_at = utc_now_iso()
    results: list[dict[str, Any]] = []
    scripted_server_port = scripted_server.port if scripted_server else None
    for task in tasks:
        task_dir = ensure_dir(output_root / task["id"])
        result = _run_coding_task(
            repo_root=repo_root,
            binary_path=binary_path,
            task_file=task_file,
            task=task,
            task_dir=task_dir,
            lane=lane,
            reference_profile=reference_profile,
            scripted_server_port=scripted_server_port,
        )
        write_json_artifact(task_dir / "result.json", result)
        results.append(result)
    finished_at = utc_now_iso()
    summary = {
        "lane": lane,
        "task_file": str(task_file),
        "binary_path": str(binary_path),
        "run_dir": str(output_root),
        "started_at": started_at,
        "finished_at": finished_at,
        "task_count": len(results),
        "passed": sum(1 for result in results if result["passed"]),
        "failed": sum(1 for result in results if not result["passed"]),
        "results": results,
        "reference_profile": reference_profile or {},
    }
    write_json_artifact(output_root / "summary.json", summary)
    (output_root / "summary.md").write_text(
        _summary_markdown("Coding Harness Summary", results, lane=lane, started_at=started_at, finished_at=finished_at),
        encoding="utf-8",
    )
    return summary


def run_runtime_cert(
    *,
    repo_root: Path,
    binary_path: Path,
    run_dir: Path,
    task_filter: str,
) -> dict[str, Any]:
    output_root = ensure_dir(run_dir / "runtime-cert")
    is_windows = subprocess.os.name == "nt"
    step_suffix = ".ps1" if is_windows else ".sh"
    shell_prefix = ["powershell", "-ExecutionPolicy", "Bypass", "-File"] if is_windows else ["bash"]
    steps = [
        {
            "id": "install-smoke",
            "argv": shell_prefix + [str(repo_root / "scripts" / f"install-smoke{step_suffix}")],
        },
        {
            "id": "support-bundle-smoke",
            "argv": shell_prefix
            + [str(repo_root / "scripts" / f"support-bundle-smoke{step_suffix}")]
            + ([str(binary_path)] if not is_windows else ["-BinaryPath", str(binary_path)]),
        },
        {
            "id": "phase1-runtime-smoke",
            "argv": shell_prefix
            + [str(repo_root / "scripts" / f"verify-phase1{step_suffix}")]
            + ([str(binary_path)] if not is_windows else ["-BinaryPath", str(binary_path)]),
        },
        {
            "id": "phase2-operator-matrix",
            "argv": shell_prefix
            + [str(repo_root / "scripts" / f"verify-phase2{step_suffix}")]
            + ([str(binary_path)] if not is_windows else ["-BinaryPath", str(binary_path)]),
        },
    ]
    patterns = [pattern.strip() for pattern in task_filter.split(",") if pattern.strip()]
    if patterns:
        steps = [
            step
            for step in steps
            if any(fnmatch.fnmatchcase(step["id"], pattern) or step["id"] == pattern for pattern in patterns)
        ]
    started_at = utc_now_iso()
    results: list[dict[str, Any]] = []
    for step in steps:
        step_dir = ensure_dir(output_root / step["id"])
        completed = run_command(step["argv"], cwd=repo_root, capture_output=True)
        stdout_path = step_dir / "stdout.txt"
        stderr_path = step_dir / "stderr.txt"
        write_text_artifact(stdout_path, completed.stdout)
        write_text_artifact(stderr_path, completed.stderr)
        result = {
            "id": step["id"],
            "command": command_label(step["argv"]),
            "passed": completed.returncode == 0,
            "summary": "passed" if completed.returncode == 0 else "step failed",
            "artifacts": {"stdout": str(stdout_path), "stderr": str(stderr_path)},
        }
        write_json_artifact(step_dir / "result.json", result)
        results.append(result)
    finished_at = utc_now_iso()
    summary = {
        "lane": "runtime-cert",
        "binary_path": str(binary_path),
        "run_dir": str(output_root),
        "started_at": started_at,
        "finished_at": finished_at,
        "task_count": len(results),
        "passed": sum(1 for result in results if result["passed"]),
        "failed": sum(1 for result in results if not result["passed"]),
        "results": results,
    }
    write_json_artifact(output_root / "summary.json", summary)
    (output_root / "summary.md").write_text(
        _summary_markdown("Runtime Certification Summary", results, lane="runtime-cert", started_at=started_at, finished_at=finished_at),
        encoding="utf-8",
    )
    return summary


def run_soak_lane(
    *,
    repo_root: Path,
    run_dir: Path,
    token: str,
    base_url: str,
    workspace: str,
) -> dict[str, Any]:
    output_root = ensure_dir(run_dir / "soak")
    is_windows = subprocess.os.name == "nt"
    step_suffix = ".ps1" if is_windows else ".sh"
    shell_prefix = ["powershell", "-ExecutionPolicy", "Bypass", "-File"] if is_windows else ["bash"]
    if not token:
        raise SystemExit("soak lane requires --token.")
    argv = shell_prefix + [str(repo_root / "scripts" / f"run-soak{step_suffix}")]
    if is_windows:
        argv += ["-Token", token, "-BaseUrl", base_url, "-Workspace", workspace, "-OutputRoot", str(output_root)]
    else:
        argv += [token, base_url, "30", "1000", workspace, str(output_root)]
    completed = run_command(argv, cwd=repo_root, capture_output=True)
    stdout_path = output_root / "stdout.txt"
    stderr_path = output_root / "stderr.txt"
    write_text_artifact(stdout_path, completed.stdout)
    write_text_artifact(stderr_path, completed.stderr)
    summary = {
        "lane": "soak",
        "run_dir": str(output_root),
        "started_at": utc_now_iso(),
        "finished_at": utc_now_iso(),
        "task_count": 1,
        "passed": 1 if completed.returncode == 0 else 0,
        "failed": 0 if completed.returncode == 0 else 1,
        "results": [
            {
                "id": "soak",
                "passed": completed.returncode == 0,
                "summary": "passed" if completed.returncode == 0 else "step failed",
                "artifacts": {"stdout": str(stdout_path), "stderr": str(stderr_path)},
            }
        ],
    }
    write_json_artifact(output_root / "summary.json", summary)
    (output_root / "summary.md").write_text(
        _summary_markdown("Soak Summary", summary["results"], lane="soak", started_at=summary["started_at"], finished_at=summary["finished_at"]),
        encoding="utf-8",
    )
    return summary
