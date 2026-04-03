#!/usr/bin/env python3
from __future__ import annotations

import argparse

from run_harness import main as run_harness_main


def main() -> int:
    parser = argparse.ArgumentParser(description="Compatibility wrapper for the canonical harness runner.")
    parser.add_argument("--task-file", default="benchmarks/coding-smoke/tasks.jsonl")
    parser.add_argument("--binary-path", default="")
    parser.add_argument("--output-root", default="")
    parser.add_argument("--bootstrap-profile", action="store_true")
    parser.add_argument("--bootstrap-root", default="")
    parser.add_argument("--daemon-port", type=int, default=0)
    parser.add_argument("--provider-port", type=int, default=0)
    args = parser.parse_args()

    forwarded = ["--lane", "analysis-smoke", "--task-file", args.task_file]
    if args.binary_path:
        forwarded += ["--binary-path", args.binary_path]
    if args.output_root:
        forwarded += ["--output-root", args.output_root]
    return run_harness_main(forwarded)


if __name__ == "__main__":
    raise SystemExit(main())
