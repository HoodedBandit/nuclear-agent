#!/usr/bin/env python3
from __future__ import annotations

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
    candidates = sorted(path for path in root.rglob("summary.json") if path.is_file())
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


def build_markdown(record: dict[str, Any]) -> str:
    lines = [
        "# Production Release Record",
        "",
        f"- generated_at: `{record['generated_at']}`",
        f"- commit_sha: `{record.get('commit_sha') or 'unknown'}`",
        f"- version: `{record.get('version') or 'unknown'}`",
        f"- platform: `{record.get('platform') or 'unknown'}`",
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
        ("runtime_cert", "runtime_cert"),
        ("coding_deterministic", "coding_deterministic"),
        ("coding_reference", "coding_reference"),
        ("analysis_smoke", "analysis_smoke"),
        ("soak", "soak"),
    ):
        current = record.get(section_key)
        if not current:
            lines.append(f"- {label}: not recorded")
            continue
        lines.append(
            f"- {label}: passed={current.get('passed')} failed={current.get('failed')} summary=`{current.get('summary_path')}`"
        )

    lines.extend(
        [
            "",
            "## Supply Chain",
            "",
            f"- sbom: `{record.get('sbom_path') or 'missing'}`",
            f"- provenance: `{record.get('provenance_path') or 'missing'}`",
            f"- signing_status: `{record.get('signing_status') or 'missing'}`",
            "",
            "## Release Inputs",
            "",
            f"- release_notes: `{record.get('notes_file')}`",
            f"- checklist: `{record.get('checklist_file')}`",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-root", default="target/release/package")
    parser.add_argument("--package-manifest", default="")
    parser.add_argument("--runtime-cert-root", default="target/verify-ga/runtime-cert")
    parser.add_argument("--runtime-cert-summary", default="")
    parser.add_argument("--coding-deterministic-root", default="target/verify-ga/coding-deterministic")
    parser.add_argument("--coding-deterministic-summary", default="")
    parser.add_argument("--coding-reference-root", default="target/finalize-release/coding-reference")
    parser.add_argument("--coding-reference-summary", default="")
    parser.add_argument("--analysis-smoke-root", default="target/harness/analysis-smoke")
    parser.add_argument("--analysis-smoke-summary", default="")
    parser.add_argument("--soak-root", default="target/soak")
    parser.add_argument("--soak-summary", default="")
    parser.add_argument("--require-coding-reference", action="store_true")
    parser.add_argument("--output-root", default="target/release-records")
    parser.add_argument("--notes-file", default="docs/ga-release-notes.md")
    parser.add_argument("--checklist-file", default="docs/release-checklist.md")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    package_manifest_path = (
        resolve_existing_path(repo_root, args.package_manifest)
        if args.package_manifest
        else find_latest_manifest(resolve_existing_path(repo_root, args.package_root))
    )
    runtime_cert_summary_path = (
        resolve_existing_path(repo_root, args.runtime_cert_summary)
        if args.runtime_cert_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.runtime_cert_root))
    )
    coding_deterministic_summary_path = (
        resolve_existing_path(repo_root, args.coding_deterministic_summary)
        if args.coding_deterministic_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.coding_deterministic_root))
    )
    coding_reference_summary_path = (
        resolve_existing_path(repo_root, args.coding_reference_summary)
        if args.coding_reference_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.coding_reference_root))
    )
    analysis_smoke_summary_path = (
        resolve_existing_path(repo_root, args.analysis_smoke_summary)
        if args.analysis_smoke_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.analysis_smoke_root))
    )
    soak_summary_path = (
        resolve_existing_path(repo_root, args.soak_summary)
        if args.soak_summary
        else find_latest_summary(resolve_existing_path(repo_root, args.soak_root))
    )

    package_manifest = read_json(package_manifest_path) if package_manifest_path and package_manifest_path.exists() else None
    runtime_cert_summary = read_json(runtime_cert_summary_path) if runtime_cert_summary_path and runtime_cert_summary_path.exists() else None
    coding_deterministic_summary = (
        read_json(coding_deterministic_summary_path)
        if coding_deterministic_summary_path and coding_deterministic_summary_path.exists()
        else None
    )
    coding_reference_summary = (
        read_json(coding_reference_summary_path)
        if coding_reference_summary_path and coding_reference_summary_path.exists()
        else None
    )
    analysis_smoke_summary = (
        read_json(analysis_smoke_summary_path)
        if analysis_smoke_summary_path and analysis_smoke_summary_path.exists()
        else None
    )
    soak_summary = read_json(soak_summary_path) if soak_summary_path and soak_summary_path.exists() else None

    output_root = resolve_existing_path(repo_root, args.output_root)
    output_root.mkdir(parents=True, exist_ok=True)
    run_dir = output_root / datetime.now().strftime("%Y%m%d-%H%M%S")
    run_dir.mkdir(parents=True, exist_ok=True)

    notes_file = resolve_existing_path(repo_root, args.notes_file)
    checklist_file = resolve_existing_path(repo_root, args.checklist_file)

    record = {
        "generated_at": utc_now_iso(),
        "commit_sha": package_manifest.get("commit_sha") if package_manifest else get_git_commit_sha(repo_root),
        "version": package_manifest.get("version") if package_manifest else "",
        "platform": package_manifest.get("platform") if package_manifest else "",
        "package": package_manifest,
        "sbom_path": package_manifest.get("sbom_path") if package_manifest else "",
        "provenance_path": package_manifest.get("provenance_path") if package_manifest else "",
        "signing_status": package_manifest.get("signing_status") if package_manifest else "",
        "runtime_cert": section(runtime_cert_summary, runtime_cert_summary_path),
        "coding_deterministic": section(coding_deterministic_summary, coding_deterministic_summary_path),
        "coding_reference": section(coding_reference_summary, coding_reference_summary_path),
        "analysis_smoke": section(analysis_smoke_summary, analysis_smoke_summary_path),
        "soak": section(soak_summary, soak_summary_path),
        "notes_file": str(notes_file),
        "checklist_file": str(checklist_file),
    }

    required_failures: list[str] = []
    if package_manifest is None:
        required_failures.append("package manifest")
    if record["runtime_cert"] is None:
        required_failures.append("runtime-cert summary")
    elif int(record["runtime_cert"].get("failed") or 0) != 0:
        required_failures.append("runtime-cert failures")
    if record["coding_deterministic"] is None:
        required_failures.append("coding-deterministic summary")
    elif int(record["coding_deterministic"].get("failed") or 0) != 0:
        required_failures.append("coding-deterministic failures")
    if args.require_coding_reference:
        if record["coding_reference"] is None:
            required_failures.append("coding-reference summary")
        elif int(record["coding_reference"].get("failed") or 0) != 0:
            required_failures.append("coding-reference failures")
    if not notes_file.exists():
        required_failures.append(f"release notes file ({notes_file})")
    if not checklist_file.exists():
        required_failures.append(f"release checklist file ({checklist_file})")
    if not record["sbom_path"]:
        required_failures.append("SBOM path")
    elif not Path(record["sbom_path"]).exists():
        required_failures.append(f"SBOM file ({record['sbom_path']})")
    if not record["provenance_path"]:
        required_failures.append("provenance path")
    elif not Path(record["provenance_path"]).exists():
        required_failures.append(f"provenance file ({record['provenance_path']})")
    if not record["signing_status"]:
        required_failures.append("signing status path")
    elif not Path(record["signing_status"]).exists():
        required_failures.append(f"signing status file ({record['signing_status']})")
    if required_failures:
        raise SystemExit("Release record requirements were not met: " + ", ".join(required_failures))

    json_path = run_dir / "release-record.json"
    markdown_path = run_dir / "release-record.md"
    write_json(json_path, record)
    markdown_path.write_text(build_markdown(record), encoding="utf-8")
    print(f"Release record written to {run_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
