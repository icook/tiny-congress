"""Tests for aggregation stage."""

import json
from pathlib import Path

from aggregate import aggregate_clusters
from io_utils import write_jsonl


def test_aggregate_clusters_creates_headlines(tmp_path: Path) -> None:
    flattened_path = tmp_path / "edus.jsonl"
    write_jsonl(
        flattened_path,
        [
            {
                "doc_id": "d1",
                "edu_id": "e1",
                "text": "Main Street parking ban keeps fire lanes open.",
                "nuclearity": "nucleus",
                "relation": None,
                "is_root": True,
            },
            {
                "doc_id": "d1",
                "edu_id": "e2",
                "text": "Residents worry about losing overnight spaces.",
                "nuclearity": "satellite",
                "relation": "elaboration",
                "parent_edu_id": "e1",
            },
        ],
    )

    clusters_payload = {
        "clusters": [
            {
                "cluster_id": 0,
                "centroid": [0.1, 0.2],
                "members": [{"doc_id": "d1", "edu_id": "e1", "is_root": True, "relation": None}],
                "satellites": [
                    {
                        "doc_id": "d1",
                        "edu_id": "e2",
                        "relation": "elaboration",
                        "attachment": "parent",
                        "score": None,
                        "cluster_id": 0,
                    }
                ],
            }
        ]
    }

    clusters_path = tmp_path / "clusters.json"
    clusters_path.write_text(json.dumps(clusters_payload))

    output_path = tmp_path / "snapshot.json"
    aggregate_clusters(
        flattened_path=flattened_path,
        clusters_with_satellites_path=clusters_path,
        output_path=output_path,
    )

    snapshot = json.loads(output_path.read_text())
    assert snapshot["counts"]["clusters"] == 1
    entry = snapshot["clusters"][0]
    assert entry["headline"] == "Main Street parking ban keeps fire lanes open."
    assert entry["satellites_by_relation"]["elaboration"][0]["text"] == (
        "Residents worry about losing overnight spaces."
    )
