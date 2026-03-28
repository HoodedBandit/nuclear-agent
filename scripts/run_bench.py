#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def resolve_binary_path(repo_root: Path, provided: Optional[str]) -> Path:
    if provided:
        candidate = Path(provided).expanduser()
        if not candidate.is_absolute():
            candidate = (repo_root / candidate).resolve()
        if candidate.exists():
            return candidate
        raise SystemExit(f"Binary not found: {candidate}")

    candidates = [
        repo_root / "target" / "debug" / "nuclear.exe",
        repo_root / "target" / "debug" / "nuclear",
        repo_root / "target" / "debug" / "autism.exe",
        repo_root / "target" / "debug" / "autism",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate.resolve()

    raise SystemExit(
        "Could not find a built nuclear/autism binary under target/debug. "
        "Build the workspace first or pass --binary-path."
    )


def load_tasks(task_file: Path) -> list[dict]:
    tasks: list[dict] = []
    for raw_line in task_file.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        task = json.loads(line)
        task_id = str(task.get("id") or "").strip()
        if not task_id:
            raise SystemExit(f"Benchmark task is missing id: {raw_line}")
        command = [str(value) for value in task.get("command") or []]
        if not command:
            raise SystemExit(f"Benchmark task '{task_id}' is missing command arguments.")
        task["id"] = task_id
        task["command"] = command
        task["description"] = str(task.get("description") or "")
        task["category"] = str(task.get("category") or "")
        task["tags"] = [str(value) for value in task.get("tags") or []]
        task["expected_exit_code"] = int(task.get("expected_exit_code", 0))
        tasks.append(task)
    return tasks


def resolve_task_cwd(repo_root: Path, task: dict) -> Path:
    cwd_value = task.get("cwd")
    if not cwd_value:
        return repo_root
    cwd_path = Path(str(cwd_value)).expanduser()
    if not cwd_path.is_absolute():
        cwd_path = repo_root / cwd_path
    return cwd_path.resolve()


def maybe_parse_json(text: str):
    stripped = text.strip()
    if not stripped:
        return None
    try:
        return json.loads(stripped)
    except json.JSONDecodeError:
        return None


def write_json(path: Path, payload) -> None:
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def build_summary_markdown(summary: dict) -> str:
    lines = [
        "# Benchmark Summary",
        "",
        f"- task_file: `{summary['task_file']}`",
        f"- binary_path: `{summary['binary_path']}`",
        f"- started_at: `{summary['started_at']}`",
        f"- finished_at: `{summary['finished_at']}`",
        f"- task_count: `{summary['task_count']}`",
        f"- passed: `{summary['passed']}`",
        f"- failed: `{summary['failed']}`",
        "",
        "| Task | Category | Passed | Exit | Duration (ms) | Model | Provider |",
        "| --- | --- | --- | --- | ---: | --- | --- |",
    ]
    for result in summary["results"]:
        lines.append(
            "| {id} | {category} | {passed} | {exit_code} | {duration_ms} | {model} | {provider_id} |".format(
                id=result["id"],
                category=result.get("category") or "-",
                passed="yes" if result["passed"] else "no",
                exit_code=result["exit_code"],
                duration_ms=result["duration_ms"],
                model=result.get("model") or "-",
                provider_id=result.get("provider_id") or "-",
            )
        )
    return "\n".join(lines) + "\n"


def run_task(task: dict, repo_root: Path, binary_path: Path, run_dir: Path) -> dict:
    task_dir = run_dir / task["id"]
    task_dir.mkdir(parents=True, exist_ok=True)

    request_path = task_dir / "request.json"
    stdout_path = task_dir / "stdout.txt"
    stderr_path = task_dir / "stderr.txt"
    result_path = task_dir / "result.json"
    write_json(request_path, task)

    cwd = resolve_task_cwd(repo_root, task)
    started_at = utc_now_iso()
    started_monotonic = time.time()
    with stdout_path.open("w", encoding="utf-8") as stdout_file, stderr_path.open(
        "w", encoding="utf-8"
    ) as stderr_file:
        completed = subprocess.run(
            [str(binary_path), *task["command"]],
            cwd=str(cwd),
            stdout=stdout_file,
            stderr=stderr_file,
            check=False,
            text=True,
        )
    finished_at = utc_now_iso()
    duration_ms = int((time.time() - started_monotonic) * 1000)

    stdout_text = stdout_path.read_text(encoding="utf-8")
    stderr_text = stderr_path.read_text(encoding="utf-8")

    result = {
        "id": task["id"],
        "description": task["description"],
        "category": task["category"],
        "tags": task["tags"],
        "cwd": str(cwd),
        "command": task["command"],
        "started_at": started_at,
        "finished_at": finished_at,
        "duration_ms": duration_ms,
        "exit_code": completed.returncode,
        "expected_exit_code": task["expected_exit_code"],
        "passed": completed.returncode == task["expected_exit_code"],
        "stdout_bytes": stdout_path.stat().st_size,
        "stderr_bytes": stderr_path.stat().st_size,
        "artifacts": {
            "request": str(request_path),
            "stdout": str(stdout_path),
            "stderr": str(stderr_path),
        },
    }

    parsed_stdout = maybe_parse_json(stdout_text)
    if parsed_stdout is not None:
        stdout_json_path = task_dir / "stdout.json"
        write_json(stdout_json_path, parsed_stdout)
        result["artifacts"]["stdout_json"] = str(stdout_json_path)
        if isinstance(parsed_stdout, dict):
            for key in ("session_id", "alias", "provider_id", "model"):
                value = parsed_stdout.get(key)
                if isinstance(value, str) and value.strip():
                    result[key] = value
            tool_events = parsed_stdout.get("tool_events")
            if isinstance(tool_events, list):
                result["tool_event_count"] = len(tool_events)
            structured_output_json = parsed_stdout.get("structured_output_json")
            if isinstance(structured_output_json, str) and structured_output_json.strip():
                parsed_structured = maybe_parse_json(structured_output_json)
                if parsed_structured is not None:
                    structured_output_path = task_dir / "structured_output.json"
                    write_json(structured_output_path, parsed_structured)
                    result["artifacts"]["structured_output"] = str(structured_output_path)

    if stderr_text.strip():
        result["stderr_preview"] = stderr_text.strip().splitlines()[:3]

    write_json(result_path, result)
    print(
        f"[{task['id']}] pass={'yes' if result['passed'] else 'no'} "
        f"exit={completed.returncode} duration_ms={duration_ms}"
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--task-file",
        default="benchmarks/coding-smoke/tasks.jsonl",
        help="Path to a JSONL benchmark task file",
    )
    parser.add_argument("--binary-path", default="", help="Path to the CLI binary to benchmark")
    parser.add_argument(
        "--output-root",
        default="",
        help="Directory where benchmark runs should be written",
    )
    args = parser.parse_args()

    script_path = Path(__file__).resolve()
    repo_root = script_path.parent.parent
    task_file = Path(args.task_file).expanduser()
    if not task_file.is_absolute():
        task_file = (repo_root / task_file).resolve()
    if not task_file.exists():
        raise SystemExit(f"Task file not found: {task_file}")

    binary_path = resolve_binary_path(repo_root, args.binary_path or None)
    output_root = Path(args.output_root).expanduser() if args.output_root else repo_root / "target" / "benchmarks"
    if not output_root.is_absolute():
        output_root = (repo_root / output_root).resolve()
    output_root.mkdir(parents=True, exist_ok=True)

    run_dir = output_root / datetime.now().strftime("%Y%m%d-%H%M%S")
    run_dir.mkdir(parents=True, exist_ok=True)

    tasks = load_tasks(task_file)
    started_at = utc_now_iso()
    results = [run_task(task, repo_root, binary_path, run_dir) for task in tasks]
    finished_at = utc_now_iso()

    summary = {
        "task_file": str(task_file),
        "binary_path": str(binary_path),
        "run_dir": str(run_dir),
        "started_at": started_at,
        "finished_at": finished_at,
        "task_count": len(results),
        "passed": sum(1 for result in results if result["passed"]),
        "failed": sum(1 for result in results if not result["passed"]),
        "results": results,
    }

    write_json(run_dir / "summary.json", summary)
    (run_dir / "summary.md").write_text(build_summary_markdown(summary), encoding="utf-8")
    print(f"Benchmark output written to {run_dir}")
    return 0 if summary["failed"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
