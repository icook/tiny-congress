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


def test_generate_docs_lmstudio_backend(monkeypatch, tmp_path: Path) -> None:
    class FakeResponse:
        def raise_for_status(self) -> None:
            return None

        def json(self) -> dict:
            content = (
                '{"doc_id":"d000001","topic_id":"topic.parking-ban-main-st","author_id":"u_0001",'
                '"text":"However the curb feels chaotic at night. Because the ban spaces out deliveries, residents sleep easier."}'
                '{"doc_id":"d000002","topic_id":"topic.bike-boulevard-elm","author_id":"u_0002",'
                '"text":"Instead of blocking bikes, the boulevard smooths traffic. However, merchants want clearer loading zones."}'
            )
            return {"choices": [{"message": {"content": content}}]}

    called = {}

    def fake_post(url: str, json: dict, timeout: int):
        called["url"] = url
        called["payload"] = json
        called["timeout"] = timeout
        return FakeResponse()

    from synth_data import requests as synth_requests

    monkeypatch.setattr(synth_requests, "post", fake_post)

    output = tmp_path / "lm.jsonl"
    generate_docs(
        output_path=output,
        count=2,
        seed=0,
        backend="lmstudio",
        lmstudio_url="http://127.0.0.1:1234/v1/chat/completions",
        lmstudio_model="openai/gpt-oss-20b",
        temperature=0.7,
        timeout=30,
    )

    docs = read_jsonl(output)
    assert len(docs) == 2
    assert docs[0]["doc_id"] == "d000001"
    assert called["payload"]["model"] == "openai/gpt-oss-20b"
    assert called["timeout"] == 30
