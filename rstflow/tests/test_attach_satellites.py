"""Tests for attaching satellites to clusters."""

import json
from pathlib import Path

import numpy as np

from attach_satellites import attach_satellites
from cluster_nucleus import cluster_nuclei
from embed import embed_edus
from io_utils import read_jsonl, write_jsonl


def _prepare_artifacts(tmp_path: Path) -> tuple[Path, Path, Path]:
    # Step 1: write flattened EDUs with both nucleus and satellites
    flattened = [
        {
            "doc_id": "d1",
            "topic_id": "t1",
            "edu_id": "e1",
            "text": "Root nucleus about parking rules.",
            "nuclearity": "nucleus",
            "relation": None,
            "span": None,
            "is_root": True,
        },
        {
            "doc_id": "d1",
            "topic_id": "t1",
            "edu_id": "e2",
            "text": "Satellite supporting details about delivery windows.",
            "nuclearity": "satellite",
            "relation": "elaboration",
            "parent_edu_id": "e1",
            "span": None,
            "is_root": False,
        },
        {
            "doc_id": "d2",
            "topic_id": "t2",
            "edu_id": "e3",
            "text": "Another nucleus discussing sidewalk space.",
            "nuclearity": "nucleus",
            "relation": None,
            "span": None,
            "is_root": True,
        },
        {
            "doc_id": "d2",
            "topic_id": "t2",
            "edu_id": "e4",
            "text": "Satellite with no parent id yet mentions sidewalk crowding.",
            "nuclearity": "satellite",
            "relation": "elaboration",
            "parent_edu_id": None,
            "span": None,
            "is_root": False,
        },
    ]
    flatten_path = tmp_path / "edus.jsonl"
    write_jsonl(flatten_path, flattened)

    embeddings_dir = tmp_path / "embeddings"

    class TinyEmbedder:
        def encode(self, sentences, **kwargs):
            vectors = []
            for sentence in sentences:
                length = len(sentence)
                vectors.append([length / 100.0, (length % 7) / 10.0])
            return np.asarray(vectors, dtype=np.float32)

    embed_edus(
        input_path=flatten_path,
        output_dir=embeddings_dir,
        model_name="tiny",
        embedder=TinyEmbedder(),
    )

    clusters_path = tmp_path / "clusters.json"
    cluster_nuclei(
        embeddings_path=embeddings_dir,
        output_path=clusters_path,
        k=2,
        seed=7,
    )

    return flatten_path, embeddings_dir, clusters_path


def test_attach_satellites_assigns_by_parent_and_nearest(tmp_path: Path) -> None:
    _, embeddings_dir, clusters_path = _prepare_artifacts(tmp_path)
    output_path = tmp_path / "clusters_with_satellites.json"

    attach_satellites(
        embeddings_dir=embeddings_dir,
        clusters_path=clusters_path,
        output_path=output_path,
    )

    payload = json.loads(output_path.read_text())

    assert payload["counts"]["satellites"] == 2
    assignments = payload["assignments"]

    parent_assignment = next(item for item in assignments if item["edu_id"] == "e2")
    assert parent_assignment["attachment"] == "parent"

    nearest_assignment = next(item for item in assignments if item["edu_id"] == "e4")
    assert nearest_assignment["attachment"] == "nearest"
    assert isinstance(nearest_assignment["score"], float)

    for cluster in payload["clusters"]:
        for sat in cluster.get("satellites", []):
            assert sat["cluster_id"] == assignments[next(i for i, a in enumerate(assignments) if a["edu_id"] == sat["edu_id"])]["cluster_id"]
