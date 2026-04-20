#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path

from harness.artifacts import create_run_dir
from harness.common import (
    allocate_local_port,
    repo_root_from_script,
    resolve_binary_path,
    sanitize_text,
    write_json_artifact,
)
from harness.evaluator import run_analysis_tasks, run_coding_tasks, run_runtime_cert, run_soak_lane
from harness.provider_adapters import SCRIPTED_MODEL, HarnessProviderServer, load_scripted_turns
from harness.tasks import (
    load_analysis_tasks,
    load_coding_tasks,
    load_provider_profile,
    merge_provider_profile,
    select_tasks,
)


DEFAULT_ANALYSIS_TASK_FILE = "harness/tasks/analysis-smoke/tasks.jsonl"
DEFAULT_CODING_TASK_FILE = "harness/tasks/coding/tasks.json"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Canonical release harness runner")
    parser.add_argument("--lane", required=True, choices=["runtime-cert", "coding-deterministic", "coding-reference", "analysis-smoke", "soak"])
    parser.add_argument("--binary-path", default="")
    parser.add_argument("--output-root", default="")
    parser.add_argument("--task-file", default="")
    parser.add_argument("--profile", default="")
    parser.add_argument("--task-filter", default="")
    parser.add_argument("--alias", default="")
    parser.add_argument("--provider-id", default="")
    parser.add_argument("--model", default="")
    parser.add_argument("--provider-kind", default="")
    parser.add_argument("--base-url", default="")
    parser.add_argument("--api-key-env", default="")
    parser.add_argument("--token", default="")
    parser.add_argument("--soak-base-url", default="http://127.0.0.1:42690")
    parser.add_argument("--workspace", default="")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    repo_root = repo_root_from_script(Path(__file__))
    binary_path = resolve_binary_path(repo_root, args.binary_path or None) if args.lane != "soak" else None

    default_output_root = repo_root / "target" / "harness" / args.lane
    output_root = (
        (repo_root / args.output_root).resolve()
        if args.output_root and not Path(args.output_root).is_absolute()
        else Path(args.output_root or default_output_root)
    )
    output_root = output_root.resolve()
    run_dir = create_run_dir(output_root)

    if args.lane == "analysis-smoke":
        task_file = Path(args.task_file or DEFAULT_ANALYSIS_TASK_FILE)
        if not task_file.is_absolute():
            task_file = (repo_root / task_file).resolve()
        tasks = select_tasks(load_analysis_tasks(task_file), args.task_filter)
        if not tasks:
            raise SystemExit("No analysis-smoke tasks matched the task filter.")
        summary = run_analysis_tasks(repo_root=repo_root, binary_path=binary_path, task_file=task_file, tasks=tasks, run_dir=run_dir)
    elif args.lane == "runtime-cert":
        summary = run_runtime_cert(repo_root=repo_root, binary_path=binary_path, run_dir=run_dir, task_filter=args.task_filter)
    elif args.lane in {"coding-deterministic", "coding-reference"}:
        task_file = Path(args.task_file or DEFAULT_CODING_TASK_FILE)
        if not task_file.is_absolute():
            task_file = (repo_root / task_file).resolve()
        tasks = select_tasks(load_coding_tasks(task_file), args.task_filter)
        if not tasks:
            raise SystemExit(f"No {args.lane} tasks matched the task filter.")

        reference_profile = None
        if args.lane == "coding-reference":
            profile_file = Path(args.profile) if args.profile else None
            if profile_file and not profile_file.is_absolute():
                profile_file = (repo_root / profile_file).resolve()
            base_profile = load_provider_profile(profile_file) if profile_file else None
            overrides = {
                "alias": args.alias,
                "provider_id": args.provider_id,
                "model": args.model,
                "provider_kind": args.provider_kind,
                "base_url": args.base_url,
                "api_key_env": args.api_key_env,
            }
            reference_profile = merge_provider_profile(base_profile, overrides)

        scripted_server = None
        if args.lane == "coding-deterministic":
            scripts = load_scripted_turns(task_file, tasks)
            scripted_server = HarnessProviderServer(
                host="127.0.0.1",
                port=allocate_local_port(),
                mode="scripted",
                default_model=SCRIPTED_MODEL,
                scripts=scripts,
                script_root=task_file.parent,
            )
            scripted_server.start()
        try:
            summary = run_coding_tasks(
                repo_root=repo_root,
                binary_path=binary_path,
                task_file=task_file,
                tasks=tasks,
                run_dir=run_dir,
                lane=args.lane,
                reference_profile=reference_profile,
                scripted_server=scripted_server,
            )
        finally:
            if scripted_server is not None:
                scripted_server.stop()
    else:
        summary = run_soak_lane(
            repo_root=repo_root,
            run_dir=run_dir,
            token=args.token,
            base_url=args.soak_base_url,
            workspace=args.workspace,
        )

    write_json_artifact(run_dir / "summary.json", summary)
    print(sanitize_text(f"Harness output written to {run_dir}"))
    return 0 if summary.get("failed", 1) == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
