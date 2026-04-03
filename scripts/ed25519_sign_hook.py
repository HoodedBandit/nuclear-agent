#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path

from nacl.signing import SigningKey


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_signing_key() -> SigningKey:
    encoded = os.environ.get("NUCLEAR_ED25519_PRIVATE_KEY", "").strip()
    if not encoded:
        raise SystemExit("NUCLEAR_ED25519_PRIVATE_KEY is required for the Ed25519 signing hook.")
    raw = base64.b64decode(encoded)
    if len(raw) == 32:
        return SigningKey(raw)
    if len(raw) == 64:
        return SigningKey(raw[:32])
    raise SystemExit("NUCLEAR_ED25519_PRIVATE_KEY must decode to 32 or 64 raw bytes.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", required=True)
    parser.add_argument("--signature", required=True)
    parser.add_argument("--manifest", required=True)
    args = parser.parse_args()

    artifact_path = Path(args.artifact).resolve()
    signature_path = Path(args.signature).resolve()
    key = load_signing_key()
    signed = key.sign(artifact_path.read_bytes())
    verify_key = key.verify_key

    payload = {
        "algorithm": "ed25519",
        "artifact": str(artifact_path),
        "artifact_sha256": sha256_file(artifact_path),
        "manifest": str(Path(args.manifest).resolve()),
        "key_id": os.environ.get("NUCLEAR_SIGNING_KEY_ID", "") or verify_key.encode().hex(),
        "public_key_base64": base64.b64encode(bytes(verify_key)).decode("ascii"),
        "signature_base64": base64.b64encode(bytes(signed.signature)).decode("ascii"),
    }
    signature_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"Signature written to {signature_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
