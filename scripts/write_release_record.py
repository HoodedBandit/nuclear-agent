#!/usr/bin/env python3
import argparse
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def resolve_existing_path(repo_root: Path, value: str) -> Path:
    candidate = Path(value).expanduser()
    if not candidate.is_absolute():
        candidate = (repo_root / candidate).resolve()
    return candidate


def find_latest_summary(root: Path | None) -> Path | None:
    if root is None or not root.exists():
        return None
    if root.is_file() and root.name == "summary.json":
        return root
    candidates = sorted(
        [path / "summary.json" for path in root.iterdir() if path.is_dir() and (path / "summary.json").exists()]
    )
    return candidates[-1] if candidates else None


def find_latest_manifest(root: Path | None) -> Path | None:
    if root is None or not root.exists():
        return None
    if root.is_file() and root.name.endswith(".manifest.json"):
        return root
    candidates = sorted(root.glob("*.manifest.json"))
    return candidates[-1] if candidates else None


def get_git_commit_sha(repo_root: Path) -> str:
    try:
        completed = subprocess.run(
            ["git", "-C", str(repo_root), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return ""
    if completed.returncode != 0:
        return ""
    return completed.stdout.strip()


def build_markdown(record: dict[str, Any]) -> str:
    lines = [
        "# Beta Release Record",
        "",
        f"- generated_at: `{record['generated_at']}`",
        f"- commit_sha: `{record.get('commit_sha') or 'unknown'}`",
        f"- version: `{record.get('version') or 'unknown'}`",
        "",
        "## Package",
        "",
    ]

    package = record.get("package")
    if package:
        lines.extend(
            [
                f"- bundle: `{package.get('name')}`",
                f"- archive: `{package.get('archive_path')}`",
                f"- checksum: `{package.get('checksum_path')}`",
                f"- archive_sha256: `{package.get('archive_sha256')}`",
                f"- readme: `{package.get('package_readme')}`",
                "",
            ]
        )
    else:
        lines.extend(["- package manifest not recorded", ""])

    lines.extend(["## Verification", ""])
    for label, section_key in (
        ("benchmark_smoke", "benchmark_smoke"),
        ("release_eval", "release_eval"),
        ("soak", "soak"),
    ):
        section = record.get(section_key)
        if not section:
            lines.append(f"- {label}: not recorded")
            continue
        lines.append(
            f"- {label}: passed={section.get('passed')} failed={section.get('failed')} summary=`{section.get('summary_path')}`"
        )

    lines.extend(
        [
            "",
            "## Compatibility Notes",
            "",
            "- Canonical command name is `nuclear`.",
            "- Legacy `autism` launcher remains compatibility-only.",
            "- Fresh Windows installs default to the canonical Nuclear install root.",
            "- Existing legacy Windows installs upgrade in place instead of moving automatically.",
            "",
            "## Deferred Risk",
            "",
            "- Add any intentionally deferred beta debt or operator caveats here before publishing.",
            "",
            "## Release Notes",
            "",
            "- Draft notes: `docs/beta-release-notes.md`",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-root", default="target/phase3/package")
    parser.add_argument("--package-manifest", default="")
    parser.add_argument("--benchmark-smoke-root", default="target/verify-workspace/benchmarks-smoke")
    parser.add_argument("--benchmark-smoke-summary", default="")
    parser.add_argument("--release-eval-root", default="target/verify-beta/release-eval")
    parser.add_argument("--release-eval-summary", default="")
    parser.add_argument("--soak-root", default="target/soak")
    parser.add_argument("--soak-summary", default="")
    parser.add_argument("--output-root", default="target/release-records")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    package_manifest_path = (
        resolve_existing_path(repo_root, args.package_manifest)
        if args.package_manifest
        else find_latest_manifest(resolve_existing_path(repo_root, args.package_root))
    )
    benchmark_smoke_summary_path = (
        resolve_existing_path(repo_root, args.benchmark_smoke_summary)
        if args.benchmark_smoke_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.benchmark_smoke_root))
    )
    release_eval_summary_path = (
        resolve_existing_path(repo_root, args.release_eval_summary)
        if args.release_eval_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.release_eval_root))
    )
    soak_summary_path = (
        resolve_existing_path(repo_root, args.soak_summary)
        if args.soak_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.soak_root))
    )

    package_manifest = read_json(package_manifest_path) if package_manifest_path and package_manifest_path.exists() else None
    benchmark_smoke_summary = (
        read_json(benchmark_smoke_summary_path)
        if benchmark_smoke_summary_path and benchmark_smoke_summary_path.exists()
        else None
    )
    release_eval_summary = (
        read_json(release_eval_summary_path)
        if release_eval_summary_path and release_eval_summary_path.exists()
        else None
    )
    soak_summary = read_json(soak_summary_path) if soak_summary_path and soak_summary_path.exists() else None

    output_root = resolve_existing_path(repo_root, args.output_root)
    output_root.mkdir(parents=True, exist_ok=True)
    run_dir = output_root / datetime.now().strftime("%Y%m%d-%H%M%S")
    run_dir.mkdir(parents=True, exist_ok=True)

    def section(summary: dict[str, Any] | None, path: Path | None) -> dict[str, Any] | None:
        if summary is None or path is None:
            return None
        return {
            "summary_path": str(path),
            "run_dir": str(path.parent),
            "passed": summary.get("passed"),
            "failed": summary.get("failed"),
            "task_count": summary.get("task_count") or summary.get("iterations"),
        }

    record = {
        "generated_at": utc_now_iso(),
        "commit_sha": package_manifest.get("commit_sha") if package_manifest else get_git_commit_sha(repo_root),
        "version": package_manifest.get("version") if package_manifest else "",
        "package": package_manifest,
        "benchmark_smoke": section(benchmark_smoke_summary, benchmark_smoke_summary_path),
        "release_eval": section(release_eval_summary, release_eval_summary_path),
        "soak": section(soak_summary, soak_summary_path),
        "notes_file": str((repo_root / "docs" / "beta-release-notes.md").resolve()),
    }

    json_path = run_dir / "release-record.json"
    markdown_path = run_dir / "release-record.md"
    write_json(json_path, record)
    markdown_path.write_text(build_markdown(record), encoding="utf-8")
    print(f"Release record written to {run_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
