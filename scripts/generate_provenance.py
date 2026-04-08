#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest-path", required=True)
    parser.add_argument("--archive-path", required=True)
    parser.add_argument("--checksum-path", required=True)
    parser.add_argument("--sbom-path", required=True)
    parser.add_argument("--output-path", required=True)
    args = parser.parse_args()

    manifest_path = Path(args.manifest_path).resolve()
    archive_path = Path(args.archive_path).resolve()
    checksum_path = Path(args.checksum_path).resolve()
    sbom_path = Path(args.sbom_path).resolve()
    output_path = Path(args.output_path).resolve()

    manifest = read_json(manifest_path)
    predicate = {
        "buildDefinition": {
            "buildType": "https://nuclear.local/build/package-release/v1",
            "externalParameters": {
                "version": manifest.get("version"),
                "platform": manifest.get("platform"),
                "bundle_name": manifest.get("name"),
            },
            "internalParameters": {
                "script": "scripts/package-release",
                "created_at": manifest.get("created_at"),
            },
            "resolvedDependencies": [
                {
                    "uri": "git+local",
                    "digest": {
                        "gitCommit": manifest.get("commit_sha") or "unknown",
                    },
                }
            ],
        },
        "runDetails": {
            "builder": {
                "id": os.environ.get("NUCLEAR_BUILDER_ID", "local://codex"),
            },
            "metadata": {
                "invocationId": utc_now_iso(),
                "startedOn": manifest.get("created_at"),
                "finishedOn": utc_now_iso(),
            },
        },
        "materials": [
            {
                "uri": str(manifest_path),
                "digest": {"sha256": sha256_file(manifest_path)},
            },
            {
                "uri": str(sbom_path),
                "digest": {"sha256": sha256_file(sbom_path)},
            },
        ],
    }

    statement = {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [
            {
                "name": archive_path.name,
                "digest": {"sha256": sha256_file(archive_path)},
            },
            {
                "name": checksum_path.name,
                "digest": {"sha256": sha256_file(checksum_path)},
            },
            {
                "name": sbom_path.name,
                "digest": {"sha256": sha256_file(sbom_path)},
            },
        ],
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": predicate,
    }

    output_path.write_text(json.dumps(statement, indent=2), encoding="utf-8")
    print(f"Provenance written to {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
