from __future__ import annotations

from datetime import datetime
from pathlib import Path
from typing import Any

from .common import ensure_dir, write_json_artifact


def create_run_dir(output_root: Path) -> Path:
    ensure_dir(output_root)
    run_dir = output_root / datetime.now().strftime("%Y%m%d-%H%M%S")
    ensure_dir(run_dir)
    return run_dir


def write_summary(run_dir: Path, summary: dict[str, Any], markdown: str) -> None:
    write_json_artifact(run_dir / "summary.json", summary)
    (run_dir / "summary.md").write_text(markdown, encoding="utf-8")
