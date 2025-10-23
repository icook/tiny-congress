"""Utilities for reading and writing JSON/JSONL pipeline artifacts."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from pathlib import Path
from typing import Any

import orjson


def _coerce_path(path: str | Path) -> Path:
    """Convert input to a resolved Path."""
    if isinstance(path, Path):
        return path
    return Path(path)


def write_jsonl(path: str | Path, rows: Iterable[Mapping[str, Any]]) -> None:
    """Write an iterable of mappings to JSON Lines format."""
    resolved_path = _coerce_path(path)
    resolved_path.parent.mkdir(parents=True, exist_ok=True)

    with resolved_path.open("wb") as handle:
        for row in rows:
            handle.write(orjson.dumps(dict(row)))
            handle.write(b"\n")


def read_jsonl(path: str | Path) -> list[dict[str, Any]]:
    """Read a JSON Lines file into a list of dictionaries."""
    resolved_path = _coerce_path(path)
    with resolved_path.open("rb") as handle:
        return [orjson.loads(line) for line in handle if line.strip()]
