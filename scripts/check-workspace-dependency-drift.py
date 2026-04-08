#!/usr/bin/env python3
from __future__ import annotations

import sys
import tomllib
from collections import defaultdict
from pathlib import Path


DEPENDENCY_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def iter_member_manifests(repo_root: Path, workspace_members: list[str]) -> list[Path]:
    manifests: list[Path] = []
    for member in workspace_members:
        for match in sorted(repo_root.glob(member)):
            manifest = match if match.name == "Cargo.toml" else match / "Cargo.toml"
            if manifest.exists():
                manifests.append(manifest)
    return manifests


def iter_dependency_tables(table: dict, prefix: str = ""):
    for section in DEPENDENCY_SECTIONS:
        value = table.get(section)
        if isinstance(value, dict):
            yield f"{prefix}{section}", value

    target = table.get("target")
    if not isinstance(target, dict):
        return

    for target_name, target_table in target.items():
        if isinstance(target_table, dict):
            yield from iter_dependency_tables(target_table, prefix=f"{prefix}target.{target_name}.")


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    root_manifest = load_toml(repo_root / "Cargo.toml")
    workspace = root_manifest["workspace"]
    workspace_dependencies = set(workspace.get("dependencies", {}).keys())
    member_manifests = iter_member_manifests(repo_root, workspace["members"])

    member_names: set[str] = set()
    parsed_members: list[tuple[Path, dict]] = []
    for manifest in member_manifests:
        data = load_toml(manifest)
        parsed_members.append((manifest, data))
        package = data.get("package", {})
        name = package.get("name")
        if isinstance(name, str):
            member_names.add(name)

    usages_by_dependency: dict[str, list[dict[str, object]]] = defaultdict(list)
    for manifest, data in parsed_members:
        member_name = data.get("package", {}).get("name", manifest.parent.name)
        for section_name, dependency_table in iter_dependency_tables(data):
            for declared_name, spec in dependency_table.items():
                actual_name = declared_name
                uses_workspace = False
                has_path = False

                if isinstance(spec, str):
                    pass
                elif isinstance(spec, dict):
                    actual_name = spec.get("package", declared_name)
                    uses_workspace = bool(spec.get("workspace"))
                    has_path = "path" in spec
                else:
                    continue

                if has_path and actual_name in member_names:
                    continue

                usages_by_dependency[actual_name].append(
                    {
                        "member": member_name,
                        "manifest": manifest,
                        "section": section_name,
                        "declared_name": declared_name,
                        "uses_workspace": uses_workspace,
                    }
                )

    violations: list[str] = []

    for dependency_name, usages in sorted(usages_by_dependency.items()):
        members = sorted({str(usage["member"]) for usage in usages})
        shared = len(members) > 1
        in_workspace = dependency_name in workspace_dependencies

        if shared and not in_workspace:
            manifests = ", ".join(members)
            violations.append(
                f"{dependency_name}: shared direct dependency used by [{manifests}] is not centralized in [workspace.dependencies]"
            )

        if in_workspace:
            for usage in usages:
                if usage["uses_workspace"]:
                    continue
                violations.append(
                    f"{dependency_name}: {usage['manifest'].relative_to(repo_root)}::{usage['section']}->{usage['declared_name']} bypasses workspace = true"
                )

    if violations:
        print("workspace dependency drift detected:")
        for violation in violations:
            print(f" - {violation}")
        return 1

    print("workspace dependency drift check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
