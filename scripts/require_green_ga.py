#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any


def build_runs_url(api_url: str, repo: str, sha: str) -> str:
    encoded_sha = urllib.parse.quote(sha, safe="")
    return f"{api_url.rstrip('/')}/repos/{repo}/actions/runs?head_sha={encoded_sha}&per_page=100"


def load_runs(api_url: str, repo: str, sha: str, token: str) -> dict[str, Any]:
    request = urllib.request.Request(
        build_runs_url(api_url, repo, sha),
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "nuclear-release-gate",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(
            f"GitHub API request failed with HTTP {error.code}: {body}"
        ) from error
    except urllib.error.URLError as error:
        raise RuntimeError(f"GitHub API request failed: {error}") from error


def list_matching_runs(
    payload: dict[str, Any],
    workflow_name: str,
    sha: str,
    branch: str | None,
    event: str | None,
) -> list[dict[str, Any]]:
    runs = []
    for run in payload.get("workflow_runs", []):
        if workflow_name and run.get("name") != workflow_name:
            continue
        if run.get("head_sha") != sha:
            continue
        if branch and run.get("head_branch") != branch:
            continue
        if event and run.get("event") != event:
            continue
        runs.append(run)
    return sorted(runs, key=run_sort_key, reverse=True)


def run_sort_key(run: dict[str, Any]) -> tuple[Any, ...]:
    return (
        run.get("created_at") or "",
        run.get("run_number") or 0,
        run.get("id") or 0,
    )


def evaluate_latest_run(
    runs: list[dict[str, Any]],
) -> tuple[str, dict[str, Any] | None]:
    if not runs:
        return ("missing", None)
    latest = runs[0]
    if latest.get("status") != "completed":
        return ("pending", latest)
    if latest.get("conclusion") == "success":
        return ("success", latest)
    return ("failure", latest)


def describe_run(run: dict[str, Any]) -> str:
    workflow_name = run.get("name", "<unknown-workflow>")
    run_number = run.get("run_number", "?")
    status = run.get("status", "?")
    conclusion = run.get("conclusion") or "-"
    url = run.get("html_url", "<no-url>")
    return f"{workflow_name} run #{run_number} ({status}/{conclusion}) {url}"


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Require a successful ga-verify run for an exact commit before releasing."
    )
    parser.add_argument("--repo", required=True, help="GitHub repository in owner/name form.")
    parser.add_argument("--sha", required=True, help="Commit SHA to validate.")
    parser.add_argument(
        "--workflow-name",
        default="ga-verify",
        help="Workflow name to require. Defaults to ga-verify.",
    )
    parser.add_argument(
        "--branch",
        default="main",
        help="Expected branch for the matching workflow run. Defaults to main.",
    )
    parser.add_argument(
        "--event",
        default="push",
        help="Expected GitHub event for the matching workflow run. Defaults to push.",
    )
    parser.add_argument(
        "--api-url",
        default=os.environ.get("GITHUB_API_URL", "https://api.github.com"),
        help="GitHub API base URL.",
    )
    parser.add_argument(
        "--token",
        default=os.environ.get("GITHUB_TOKEN", ""),
        help="GitHub token. Defaults to GITHUB_TOKEN.",
    )
    parser.add_argument(
        "--wait",
        action="store_true",
        help="Poll until the latest matching run completes successfully or fails.",
    )
    parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=3600,
        help="Wait timeout when --wait is set. Defaults to 3600.",
    )
    parser.add_argument(
        "--poll-interval-seconds",
        type=int,
        default=15,
        help="Polling interval when --wait is set. Defaults to 15.",
    )
    return parser.parse_args(argv)


def require_success(args: argparse.Namespace) -> int:
    if not args.token:
        raise RuntimeError("GitHub token is required. Set GITHUB_TOKEN or pass --token.")

    deadline = time.monotonic() + args.timeout_seconds if args.wait else None
    while True:
        payload = load_runs(args.api_url, args.repo, args.sha, args.token)
        runs = list_matching_runs(
            payload,
            workflow_name=args.workflow_name,
            sha=args.sha,
            branch=args.branch,
            event=args.event,
        )
        outcome, run = evaluate_latest_run(runs)
        if outcome == "success":
            print(f"Verified release gate: {describe_run(run)}")
            return 0
        if not args.wait:
            if outcome == "missing":
                raise RuntimeError(
                    f"No matching successful {args.workflow_name} run exists for {args.sha} on {args.branch}."
                )
            if outcome == "pending":
                raise RuntimeError(
                    f"The latest matching {args.workflow_name} run is still in progress: {describe_run(run)}"
                )
            raise RuntimeError(
                f"The latest matching {args.workflow_name} run did not succeed: {describe_run(run)}"
            )
        if outcome == "failure":
            raise RuntimeError(
                f"The latest matching {args.workflow_name} run failed: {describe_run(run)}"
            )
        if deadline is not None and time.monotonic() >= deadline:
            if outcome == "missing":
                raise RuntimeError(
                    f"Timed out waiting for a matching {args.workflow_name} run for {args.sha}."
                )
            raise RuntimeError(
                f"Timed out waiting for {args.workflow_name} to finish successfully: {describe_run(run)}"
            )
        if outcome == "missing":
            print(
                f"Waiting for {args.workflow_name} run for {args.sha} on {args.branch}...",
                file=sys.stderr,
            )
        else:
            print(
                f"Waiting for successful completion: {describe_run(run)}",
                file=sys.stderr,
            )
        time.sleep(args.poll_interval_seconds)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        return require_success(args)
    except RuntimeError as error:
        print(f"release gate failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
