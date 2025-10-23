"""Tests for JSONL helpers."""

from pathlib import Path

from io_utils import read_jsonl, write_jsonl


def test_write_and_read_jsonl_roundtrip(tmp_path: Path) -> None:
    target = tmp_path / "artifacts" / "sample.jsonl"
    payload = [{"id": 1, "text": "hello"}, {"id": 2, "text": "world"}]

    write_jsonl(target, payload)
    read_back = read_jsonl(target)

    assert read_back == payload


def test_read_jsonl_ignores_blank_lines(tmp_path: Path) -> None:
    target = tmp_path / "data.jsonl"
    # Write manually with blank lines to ensure parser drops them.
    target.write_text('{"id": 1}\n\n{"id": 2}\n')

    assert read_jsonl(target) == [{"id": 1}, {"id": 2}]
