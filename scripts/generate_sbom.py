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


def cargo_metadata(repo_root: Path) -> dict[str, Any]:
    completed = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=str(repo_root),
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return json.loads(completed.stdout)


def npm_packages(repo_root: Path) -> list[dict[str, str]]:
    lock_path = repo_root / "package-lock.json"
    if not lock_path.exists():
        return []
    lock = read_json(lock_path)
    packages = []
    for package_path, payload in (lock.get("packages") or {}).items():
        if not isinstance(payload, dict):
            continue
        name = payload.get("name")
        version = payload.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            continue
        packages.append(
            {
                "name": name,
                "version": version,
                "path": package_path or ".",
                "license": payload.get("license") or "NOASSERTION",
            }
        )
    return packages


def spdx_package(package: dict[str, str], *, spdx_id: str, package_url: str, download_location: str) -> dict[str, Any]:
    return {
        "name": package["name"],
        "SPDXID": spdx_id,
        "versionInfo": package["version"],
        "downloadLocation": download_location,
        "licenseConcluded": package.get("license") or "NOASSERTION",
        "licenseDeclared": package.get("license") or "NOASSERTION",
        "filesAnalyzed": False,
        "externalRefs": [
            {
                "referenceCategory": "PACKAGE-MANAGER",
                "referenceType": "purl",
                "referenceLocator": package_url,
            }
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", required=True)
    parser.add_argument("--bundle-name", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", required=True)
    parser.add_argument("--output-path", required=True)
    args = parser.parse_args()

    repo_root = Path(args.repo_root).resolve()
    output_path = Path(args.output_path).resolve()
    output_path.parent.mkdir(parents=True, exist_ok=True)

    metadata = cargo_metadata(repo_root)
    rust_packages = []
    for package in metadata.get("packages", []):
        if not isinstance(package, dict):
            continue
        source = package.get("source")
        if not isinstance(source, str) or not source:
            source = package.get("manifest_path") or "NOASSERTION"
        rust_packages.append(
            {
                "name": package.get("name", "unknown"),
                "version": package.get("version", "unknown"),
                "license": package.get("license") or "NOASSERTION",
                "download": source,
            }
        )

    js_packages = npm_packages(repo_root)

    packages: list[dict[str, Any]] = [
        {
            "name": args.bundle_name,
            "SPDXID": "SPDXRef-Package-Bundle",
            "versionInfo": args.version,
            "downloadLocation": "NOASSERTION",
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "filesAnalyzed": False,
        }
    ]
    relationships = []

    for index, package in enumerate(rust_packages, start=1):
        spdx_id = f"SPDXRef-RustPackage-{index}"
        packages.append(
            spdx_package(
                package,
                spdx_id=spdx_id,
                package_url=f"pkg:cargo/{package['name']}@{package['version']}",
                download_location=package["download"],
            )
        )
        relationships.append(
            {
                "spdxElementId": "SPDXRef-Package-Bundle",
                "relationshipType": "CONTAINS",
                "relatedSpdxElement": spdx_id,
            }
        )

    for index, package in enumerate(js_packages, start=1):
        spdx_id = f"SPDXRef-NpmPackage-{index}"
        packages.append(
            spdx_package(
                package,
                spdx_id=spdx_id,
                package_url=f"pkg:npm/{package['name']}@{package['version']}",
                download_location=package["path"],
            )
        )
        relationships.append(
            {
                "spdxElementId": "SPDXRef-Package-Bundle",
                "relationshipType": "CONTAINS",
                "relatedSpdxElement": spdx_id,
            }
        )

    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"{args.bundle_name}-sbom",
        "documentNamespace": f"https://nuclear.local/sbom/{args.bundle_name}/{utc_now_iso()}",
        "creationInfo": {
            "created": utc_now_iso(),
            "creators": ["Tool: scripts/generate_sbom.py"],
        },
        "documentDescribes": ["SPDXRef-Package-Bundle"],
        "packages": packages,
        "relationships": relationships,
        "annotations": [
            {
                "annotationDate": utc_now_iso(),
                "annotationType": "OTHER",
                "annotator": "Tool: scripts/generate_sbom.py",
                "comment": f"platform={args.platform}",
            }
        ],
    }

    output_path.write_text(json.dumps(document, indent=2), encoding="utf-8")
    print(f"SBOM written to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
