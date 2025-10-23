"""Tests for embedding stage."""

from pathlib import Path

import numpy as np

from embed import embed_edus
from io_utils import read_jsonl, write_jsonl


class FakeEmbedder:
    """Deterministic embedder for tests."""

    def __init__(self, dimension: int = 4):
        self.dimension = dimension

    def encode(self, sentences, **kwargs):
        vectors = []
        for sentence in sentences:
            base = len(sentence) % (self.dimension + 3)
            row = [(base + offset) / 10.0 for offset in range(self.dimension)]
            vectors.append(row)
        return np.asarray(vectors, dtype=np.float32)


def _write_flattened(tmp_path: Path) -> Path:
    data = [
        {
            "doc_id": "d000001",
            "topic_id": "topic.parking-ban-main-st",
            "edu_id": "e01",
            "text": "Root claim about parking.",
            "nuclearity": "nucleus",
            "relation": None,
            "parent_edu_id": None,
            "is_root": True,
        },
        {
            "doc_id": "d000001",
            "topic_id": "topic.parking-ban-main-st",
            "edu_id": "e02",
            "text": "Supporting detail referencing the curb.",
            "nuclearity": "satellite",
            "relation": "elaboration",
            "parent_edu_id": "e01",
            "is_root": False,
        },
    ]
    input_path = tmp_path / "edus.jsonl"
    write_jsonl(input_path, data)
    return input_path


def test_embed_edus_writes_arrays(tmp_path: Path) -> None:
    input_path = _write_flattened(tmp_path)
    output_dir = tmp_path / "embeddings"

    embed_edus(
        input_path=input_path,
        output_dir=output_dir,
        model_name="fake-model",
        embedder=FakeEmbedder(dimension=3),
    )

    nucleus = np.load(output_dir / "nucleus.npy")
    satellite = np.load(output_dir / "satellite.npy")

    assert nucleus.shape == (1, 3)
    assert satellite.shape == (1, 3)

    index = read_jsonl(output_dir / "index.jsonl")[0]
    assert index["model"] == "fake-model"
    assert index["counts"]["nucleus"] == 1
    assert index["counts"]["satellite"] == 1
    assert index["nucleus"][0]["is_root"] is True
    assert index["satellite"][0]["parent_edu_id"] == "e01"


def test_embed_edus_handles_empty_category(tmp_path: Path) -> None:
    data = [
        {
            "doc_id": "d000001",
            "topic_id": "topic.parking-ban-main-st",
            "edu_id": "e01",
            "text": "Only nucleus present.",
            "nuclearity": "nucleus",
            "relation": None,
        }
    ]
    input_path = tmp_path / "single.jsonl"
    write_jsonl(input_path, data)
    output_dir = tmp_path / "embeddings"

    embed_edus(
        input_path=input_path,
        output_dir=output_dir,
        model_name="fake-model",
        embedder=FakeEmbedder(dimension=2),
    )

    nucleus = np.load(output_dir / "nucleus.npy")
    satellite = np.load(output_dir / "satellite.npy")

    assert nucleus.shape == (1, 2)
    assert satellite.shape[0] == 0

    index = read_jsonl(output_dir / "index.jsonl")[0]
    assert index["counts"]["satellite"] == 0
