from __future__ import annotations

import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import require_green_ga


class RequireGreenGATests(unittest.TestCase):
    def test_list_matching_runs_filters_by_workflow_sha_branch_and_event(self) -> None:
        payload = {
            "workflow_runs": [
                {
                    "id": 10,
                    "run_number": 3,
                    "name": "ga-verify",
                    "head_sha": "abc123",
                    "head_branch": "main",
                    "event": "push",
                    "created_at": "2026-04-18T10:00:00Z",
                },
                {
                    "id": 11,
                    "run_number": 4,
                    "name": "ga-verify",
                    "head_sha": "abc123",
                    "head_branch": "feature",
                    "event": "push",
                    "created_at": "2026-04-18T11:00:00Z",
                },
                {
                    "id": 12,
                    "run_number": 5,
                    "name": "finalize-release",
                    "head_sha": "abc123",
                    "head_branch": "main",
                    "event": "workflow_dispatch",
                    "created_at": "2026-04-18T12:00:00Z",
                },
            ]
        }

        runs = require_green_ga.list_matching_runs(
            payload,
            workflow_name="ga-verify",
            sha="abc123",
            branch="main",
            event="push",
        )

        self.assertEqual([run["id"] for run in runs], [10])

    def test_list_matching_runs_returns_newest_first(self) -> None:
        payload = {
            "workflow_runs": [
                {
                    "id": 10,
                    "run_number": 3,
                    "name": "ga-verify",
                    "head_sha": "abc123",
                    "head_branch": "main",
                    "event": "push",
                    "created_at": "2026-04-18T10:00:00Z",
                },
                {
                    "id": 11,
                    "run_number": 4,
                    "name": "ga-verify",
                    "head_sha": "abc123",
                    "head_branch": "main",
                    "event": "push",
                    "created_at": "2026-04-18T11:00:00Z",
                },
            ]
        }

        runs = require_green_ga.list_matching_runs(
            payload,
            workflow_name="ga-verify",
            sha="abc123",
            branch="main",
            event="push",
        )

        self.assertEqual([run["id"] for run in runs], [11, 10])

    def test_evaluate_latest_run_prefers_success_only_when_latest_completed_run_is_green(self) -> None:
        runs = [
            {
                "id": 11,
                "run_number": 4,
                "name": "ga-verify",
                "status": "completed",
                "conclusion": "success",
                "html_url": "https://example.com/11",
            }
        ]

        outcome, run = require_green_ga.evaluate_latest_run(runs)

        self.assertEqual(outcome, "success")
        self.assertEqual(run["id"], 11)

    def test_evaluate_latest_run_reports_pending_when_newer_run_is_in_progress(self) -> None:
        runs = [
            {
                "id": 12,
                "run_number": 5,
                "name": "ga-verify",
                "status": "in_progress",
                "conclusion": None,
                "html_url": "https://example.com/12",
            },
            {
                "id": 11,
                "run_number": 4,
                "name": "ga-verify",
                "status": "completed",
                "conclusion": "success",
                "html_url": "https://example.com/11",
            },
        ]

        outcome, run = require_green_ga.evaluate_latest_run(runs)

        self.assertEqual(outcome, "pending")
        self.assertEqual(run["id"], 12)

    def test_evaluate_latest_run_reports_failure_when_latest_completed_run_failed(self) -> None:
        runs = [
            {
                "id": 12,
                "run_number": 5,
                "name": "ga-verify",
                "status": "completed",
                "conclusion": "failure",
                "html_url": "https://example.com/12",
            }
        ]

        outcome, run = require_green_ga.evaluate_latest_run(runs)

        self.assertEqual(outcome, "failure")
        self.assertEqual(run["id"], 12)

    def test_evaluate_latest_run_reports_missing_when_no_runs_exist(self) -> None:
        outcome, run = require_green_ga.evaluate_latest_run([])

        self.assertEqual(outcome, "missing")
        self.assertIsNone(run)


if __name__ == "__main__":
    unittest.main()
