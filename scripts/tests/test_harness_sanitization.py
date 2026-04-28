from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from harness.common import (
    REDACTED,
    assert_no_sensitive_artifact_text,
    run_command,
    sanitize_artifact_payload,
    sanitize_text,
    write_json_artifact,
    write_json_config_raw,
    write_text_artifact,
)
from harness.evaluator import _artifact_reference_profile


class HarnessSanitizationTests(unittest.TestCase):
    def test_sanitize_artifact_payload_redacts_sensitive_fields(self) -> None:
        payload = {
            "access_token": "access-secret",
            "nested": {
                "refresh_token": "refresh-secret",
            },
            "message": "Bearer sk-live-123456",
        }

        sanitized = sanitize_artifact_payload(payload)
        encoded = json.dumps(sanitized)

        self.assertNotIn("access-secret", encoded)
        self.assertNotIn("refresh-secret", encoded)
        self.assertNotIn("sk-live-123456", encoded)
        self.assertIn("[REDACTED]", encoded)

    def test_sanitize_text_redacts_console_token_patterns(self) -> None:
        rendered = sanitize_text(
            "authorization=Bearer sk-live-123456 refresh_token=refresh-secret -Token plain-secret jwt=eyJhbGciOiJIUzI1Ni.eyJzdWIiOiIxMjM0NTYifQ.signature"
        )

        self.assertNotIn("sk-live-123456", rendered)
        self.assertNotIn("refresh-secret", rendered)
        self.assertNotIn("plain-secret", rendered)
        self.assertNotIn("eyJhbGciOiJIUzI1Ni", rendered)
        self.assertIn("[REDACTED]", rendered)

    def test_write_json_artifact_redacts_before_persisting(self) -> None:
        root = Path(self.id()).with_suffix("")
        output_dir = Path.cwd() / "target" / "scripts-tests" / root
        output_dir.mkdir(parents=True, exist_ok=True)
        artifact_path = output_dir / "artifact.json"

        write_json_artifact(
            artifact_path,
            {
                "api_key": "sk-live-123456",
                "password": "password-secret",
                "access_token": "access-secret",
                "nested": {
                    "refresh_token": "refresh-secret",
                    "id_token": "id-secret",
                    "message": "Authorization: Bearer ghp_secret123456",
                },
                "note": "refresh_token=refresh-secret --api-key cli-secret jwt=eyJhbGciOiJIUzI1Ni.eyJzdWIiOiIxMjM0NTYifQ.signature",
            },
        )

        content = artifact_path.read_text(encoding="utf-8")
        self.assertNotIn("sk-live-123456", content)
        self.assertNotIn("password-secret", content)
        self.assertNotIn("access-secret", content)
        self.assertNotIn("refresh-secret", content)
        self.assertNotIn("id-secret", content)
        self.assertNotIn("ghp_secret123456", content)
        self.assertNotIn("cli-secret", content)
        self.assertNotIn("eyJhbGciOiJIUzI1Ni", content)
        self.assertIn("[REDACTED]", content)

    def test_artifact_text_guard_rejects_unredacted_sensitive_content(self) -> None:
        with self.assertRaises(ValueError):
            assert_no_sensitive_artifact_text(
                json.dumps(
                    {
                        "password": "plain-secret",
                        "note": "Bearer sk-live-123456",
                    },
                    indent=2,
                )
            )

    def test_write_json_artifact_fails_closed_before_overwrite(self) -> None:
        root = Path(self.id()).with_suffix("")
        output_dir = Path.cwd() / "target" / "scripts-tests" / root
        output_dir.mkdir(parents=True, exist_ok=True)
        artifact_path = output_dir / "artifact-existing.json"
        artifact_path.write_text("existing-content", encoding="utf-8")

        with (
            patch("harness.common.sanitize_artifact_payload", side_effect=lambda payload: payload),
            patch("harness.common.sanitize_text", side_effect=lambda content: content),
        ):
            with self.assertRaises(ValueError):
                write_json_artifact(
                    artifact_path,
                    {
                        "password": "plain-secret",
                    },
                )

        self.assertEqual(artifact_path.read_text(encoding="utf-8"), "existing-content")

    def test_artifact_writer_is_the_explicit_sensitive_result_writer(self) -> None:
        root = Path(self.id()).with_suffix("")
        output_dir = Path.cwd() / "target" / "scripts-tests" / root
        output_dir.mkdir(parents=True, exist_ok=True)
        artifact_path = output_dir / "artifact-default.json"

        write_json_artifact(
            artifact_path,
            {
                "access_token": "access-secret",
                "note": "Bearer sk-live-123456",
            },
        )

        content = artifact_path.read_text(encoding="utf-8")
        self.assertNotIn("access-secret", content)
        self.assertNotIn("sk-live-123456", content)
        self.assertIn("[REDACTED]", content)

    def test_reference_profile_artifact_never_stores_raw_api_key_env(self) -> None:
        profile = _artifact_reference_profile(
            {
                "alias": "main",
                "provider_id": "harness-reference",
                "model": "model-a",
                "provider_kind": "openai-compatible",
                "base_url": "https://example.test/v1",
                "api_key_env": "REAL_PROVIDER_API_KEY",
            }
        )

        self.assertEqual(profile["api_key_env"], REDACTED)
        self.assertEqual(profile["provider_id"], "harness-reference")

    def test_write_json_config_raw_preserves_config_values(self) -> None:
        root = Path(self.id()).with_suffix("")
        output_dir = Path.cwd() / "target" / "scripts-tests" / root
        output_dir.mkdir(parents=True, exist_ok=True)
        config_path = output_dir / "config.json"

        write_json_config_raw(
            config_path,
            {
                "daemon_token": "keep-for-config-tests",
                "provider": {"api_key": "configured-key"},
            },
        )

        content = config_path.read_text(encoding="utf-8")
        self.assertIn("keep-for-config-tests", content)
        self.assertIn("configured-key", content)

    def test_write_text_artifact_redacts_raw_command_output(self) -> None:
        root = Path(self.id()).with_suffix("")
        output_dir = Path.cwd() / "target" / "scripts-tests" / root
        output_dir.mkdir(parents=True, exist_ok=True)
        stdout_path = output_dir / "stdout.txt"

        write_text_artifact(
            stdout_path,
            "api_key=sk-live-123456\nAuthorization: Bearer ghp_secret123456\njwt=eyJhbGciOiJIUzI1Ni.eyJzdWIiOiIxMjM0NTYifQ.signature",
        )

        content = stdout_path.read_text(encoding="utf-8")
        self.assertNotIn("sk-live-123456", content)
        self.assertNotIn("ghp_secret123456", content)
        self.assertNotIn("eyJhbGciOiJIUzI1Ni", content)
        self.assertIn("[REDACTED]", content)

    def test_run_command_failure_redacts_captured_output(self) -> None:
        with self.assertRaises(RuntimeError) as raised:
            run_command(
                [
                    sys.executable,
                    "-c",
                    "import sys; print('api_key=sk-live-123456'); print('Bearer ghp_secret123456', file=sys.stderr); sys.exit(7)",
                ],
                cwd=Path.cwd(),
                check=True,
            )

        message = str(raised.exception)
        self.assertNotIn("sk-live-123456", message)
        self.assertNotIn("ghp_secret123456", message)
        self.assertIn("[REDACTED]", message)


if __name__ == "__main__":
    unittest.main()
