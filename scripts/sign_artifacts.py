#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
from pathlib import Path
from typing import Any


def parse_command(value: str) -> list[str]:
    parts = shlex.split(value)
    if not parts:
        raise SystemExit("signing command was empty")
    return parts


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest-path", required=True)
    parser.add_argument("--artifacts", nargs="+", required=True)
    parser.add_argument("--status-path", required=True)
    args = parser.parse_args()

    hook = os.environ.get("NUCLEAR_SIGNING_HOOK", "").strip()
    signing_key_id = os.environ.get("NUCLEAR_SIGNING_KEY_ID", "").strip()
    manifest_path = Path(args.manifest_path).resolve()
    status_path = Path(args.status_path).resolve()

    status: dict[str, Any] = {
        "enabled": False,
        "hook": hook,
        "key_id": signing_key_id or None,
        "signatures": {},
    }

    if not hook:
        status["reason"] = "NUCLEAR_SIGNING_HOOK is not configured"
        status_path.write_text(json.dumps(status, indent=2), encoding="utf-8")
        print(f"Signing skipped: {status['reason']}")
        return 0

    command = parse_command(hook)
    status["enabled"] = True
    for artifact in args.artifacts:
        artifact_path = Path(artifact).resolve()
        signature_path = artifact_path.with_name(f"{artifact_path.name}.sig")
        completed = subprocess.run(
            [
                *command,
                "--artifact",
                str(artifact_path),
                "--signature",
                str(signature_path),
                "--manifest",
                str(manifest_path),
            ],
            check=False,
            text=True,
            capture_output=True,
        )
        if completed.returncode != 0:
            raise SystemExit(
                f"Signing hook failed for {artifact_path} ({completed.returncode})\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
            )
        if not signature_path.exists():
            raise SystemExit(f"Signing hook did not create {signature_path}")
        status["signatures"][artifact_path.name] = str(signature_path)

    status_path.write_text(json.dumps(status, indent=2), encoding="utf-8")
    print(f"Signing metadata written to {status_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
