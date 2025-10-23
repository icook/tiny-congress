"""Aggregate nucleus and satellite information into bullet summaries."""

from __future__ import annotations

import json
from collections import defaultdict
from pathlib import Path
from typing import Any

try:  # pragma: no cover
    from .io_utils import read_jsonl
except ImportError:  # pragma: no cover
    from io_utils import read_jsonl  # type: ignore


def _load_flattened(flattened_path: Path) -> dict[tuple[str, str], dict[str, Any]]:
    rows = read_jsonl(flattened_path)
    return {(row["doc_id"], row["edu_id"]): row for row in rows}


def aggregate_clusters(
    flattened_path: Path,
    clusters_with_satellites_path: Path,
    output_path: Path,
) -> None:
    """Aggregate per-cluster data into the final snapshot structure."""
    flattened_lookup = _load_flattened(flattened_path)

    with clusters_with_satellites_path.open("r", encoding="utf-8") as handle:
        clusters_payload = json.load(handle)

    aggregated_clusters: list[dict[str, Any]] = []

    for cluster in clusters_payload["clusters"]:
        cluster_id = cluster["cluster_id"]
        nuclei_entries = []
        for member in cluster["members"]:
            key = (member["doc_id"], member["edu_id"])
            source = flattened_lookup.get(key, {})
            nuclei_entries.append(
                {
                    "doc_id": member["doc_id"],
                    "edu_id": member["edu_id"],
                    "text": source.get("text"),
                    "relation": member.get("relation"),
                    "is_root": member.get("is_root", False),
                    "topic_id": member.get("topic_id"),
                }
            )

        root_candidates = [entry for entry in nuclei_entries if entry["is_root"] and entry["text"]]
        headline = root_candidates[0]["text"] if root_candidates else (
            nuclei_entries[0]["text"] if nuclei_entries and nuclei_entries[0]["text"] else None
        )

        satellites_by_relation: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for sat in cluster.get("satellites", []):
            key = (sat["doc_id"], sat["edu_id"])
            source = flattened_lookup.get(key, {})
            relation = sat.get("relation") or "unspecified"
            satellites_by_relation[relation].append(
                {
                    "doc_id": sat["doc_id"],
                    "edu_id": sat["edu_id"],
                    "text": source.get("text"),
                    "attachment": sat.get("attachment"),
                    "score": sat.get("score"),
                    "topic_id": sat.get("topic_id"),
                }
            )

        aggregated_clusters.append(
            {
                "cluster_id": cluster_id,
                "headline": headline,
                "nuclei": nuclei_entries,
                "satellites_by_relation": dict(satellites_by_relation),
                "commonality": len(nuclei_entries),
                "satellite_count": sum(len(v) for v in satellites_by_relation.values()),
                "centroid": cluster.get("centroid"),
            }
        )

    aggregated_clusters.sort(key=lambda c: (-c["commonality"], -c["satellite_count"]))

    payload = {
        "counts": {
            "clusters": len(aggregated_clusters),
            "total_nuclei": sum(cluster["commonality"] for cluster in aggregated_clusters),
            "total_satellites": sum(cluster["satellite_count"] for cluster in aggregated_clusters),
        },
        "clusters": aggregated_clusters,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)
