"""Tests for nucleus clustering."""

import json
from pathlib import Path

import numpy as np

from cluster_nucleus import cluster_nuclei
from io_utils import write_jsonl


def _create_embeddings(tmp_path: Path) -> Path:
    embeddings_dir = tmp_path / "embeddings"
    embeddings_dir.mkdir()

    vectors = np.asarray(
        [
            [0.0, 0.0],
            [0.1, 0.0],
            [2.0, 2.0],
            [2.1, 2.2],
        ],
        dtype=np.float32,
    )
    np.save(embeddings_dir / "nucleus.npy", vectors)

    index_payload = {
        "model": "fake-model",
        "device": "cpu",
        "dimension": 2,
        "counts": {"nucleus": 4, "satellite": 0},
        "nucleus": [
            {"doc_id": "d1", "edu_id": "e1", "topic_id": "t1", "relation": None, "is_root": True},
            {"doc_id": "d2", "edu_id": "e2", "topic_id": "t1", "relation": "sequence", "is_root": False},
            {"doc_id": "d3", "edu_id": "e3", "topic_id": "t2", "relation": None, "is_root": True},
            {"doc_id": "d4", "edu_id": "e4", "topic_id": "t2", "relation": "sequence", "is_root": False},
        ],
        "satellite": [],
    }
    write_jsonl(embeddings_dir / "index.jsonl", [index_payload])
    return embeddings_dir


def test_cluster_nuclei_writes_clusters(tmp_path: Path) -> None:
    embeddings_dir = _create_embeddings(tmp_path)
    output_path = tmp_path / "clusters.json"

    cluster_nuclei(embeddings_path=embeddings_dir, output_path=output_path, k=2, seed=42)

    assert output_path.exists()
    payload = json.loads(output_path.read_text())

    assert payload["model"]["name"] == "kmeans"
    assert payload["counts"]["clusters"] == 2
    assert len(payload["clusters"]) == 2

    sizes = sorted(cluster["size"] for cluster in payload["clusters"])
    assert sizes == [2, 2]

    cluster_members = [member for cluster in payload["clusters"] for member in cluster["members"]]
    assert {m["edu_id"] for m in cluster_members} == {"e1", "e2", "e3", "e4"}


def test_cluster_nuclei_rejects_invalid_k(tmp_path: Path) -> None:
    embeddings_dir = _create_embeddings(tmp_path)
    output_path = tmp_path / "clusters.json"

    try:
        cluster_nuclei(embeddings_path=embeddings_dir, output_path=output_path, k=0, seed=42)
    except ValueError as exc:
        assert "must be greater" in str(exc)
    else:  # pragma: no cover
        raise AssertionError("Expected ValueError for k=0")

    try:
        cluster_nuclei(embeddings_path=embeddings_dir, output_path=output_path, k=10, seed=42)
    except ValueError as exc:
        assert "exceeds number of nuclei" in str(exc)
    else:  # pragma: no cover
        raise AssertionError("Expected ValueError for k>n")
