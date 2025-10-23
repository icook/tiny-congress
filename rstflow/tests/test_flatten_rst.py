"""Tests for flattening RST trees into EDU rows."""

from pathlib import Path

from flatten_rst import flatten_trees
from io_utils import read_jsonl, write_jsonl
from schemas.rst import RSTParseResult


def test_flatten_trees_expands_edus(tmp_path: Path) -> None:
    input_path = tmp_path / "rst.jsonl"
    output_path = tmp_path / "edus.jsonl"

    parse = RSTParseResult(
        edus=[
            {"edu_id": "e01", "text": "Root claim"},
            {"edu_id": "e02", "text": "Supporting detail"},
        ],
        relations=[
            {
                "child_id": "e02",
                "parent_id": "e01",
                "relation": "elaboration",
                "nuclearity": "satellite",
            }
        ],
        root_edu="e01",
    )
    write_jsonl(
        input_path,
        [
            {
                "doc_id": "d000001",
                "topic_id": "topic.parking-ban-main-st",
                "rst": parse.model_dump(),
            }
        ],
    )

    flatten_trees(input_path=input_path, output_path=output_path)

    rows = read_jsonl(output_path)
    assert len(rows) == 2

    root_row = next(row for row in rows if row["edu_id"] == "e01")
    child_row = next(row for row in rows if row["edu_id"] == "e02")

    assert root_row["is_root"] is True
    assert root_row["relation"] is None
    assert root_row["nuclearity"] == "nucleus"

    assert child_row["parent_edu_id"] == "e01"
    assert child_row["relation"] == "elaboration"
    assert child_row["nuclearity"] == "satellite"
