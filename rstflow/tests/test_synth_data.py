"""Tests for synthetic data generation."""

from pathlib import Path

import pytest

from io_utils import read_jsonl
from synth_data import CONNECTORS, generate_docs


def test_generate_docs_creates_expected_count(tmp_path: Path) -> None:
    output = tmp_path / "docs.jsonl"
    generate_docs(output_path=output, count=5, seed=42)

    docs = read_jsonl(output)
    assert len(docs) == 5
    assert docs[0]["doc_id"] == "d000001"
    assert docs[-1]["doc_id"] == "d000005"

    connector_hits = [
        any(connector in doc["text"] for connector in CONNECTORS) for doc in docs
    ]
    assert all(connector_hits)


def test_generate_docs_deterministic_seed(tmp_path: Path) -> None:
    output_one = tmp_path / "first.jsonl"
    output_two = tmp_path / "second.jsonl"

    generate_docs(output_one, count=3, seed=7)
    generate_docs(output_two, count=3, seed=7)

    assert read_jsonl(output_one) == read_jsonl(output_two)


def test_generate_docs_negative_count(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        generate_docs(tmp_path / "out.jsonl", count=-1)
