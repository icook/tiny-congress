"""Attach satellite EDUs to nucleus clusters."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Literal

import numpy as np

try:  # pragma: no cover
    from .io_utils import read_jsonl
except ImportError:  # pragma: no cover
    from io_utils import read_jsonl  # type: ignore


def _load_embeddings(embeddings_dir: Path) -> tuple[np.ndarray, np.ndarray, dict]:
    nucleus = np.load(embeddings_dir / "nucleus.npy")
    satellite = np.load(embeddings_dir / "satellite.npy")
    index = read_jsonl(embeddings_dir / "index.jsonl")[0]
    return nucleus, satellite, index


def _load_clusters(clusters_path: Path) -> dict:
    with clusters_path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _normalize_matrix(matrix: np.ndarray) -> np.ndarray:
    if not matrix.size:
        return matrix
    norms = np.linalg.norm(matrix, axis=1, keepdims=True)
    norms = np.where(norms == 0, 1.0, norms)
    return matrix / norms


def attach_satellites(
    embeddings_dir: Path,
    clusters_path: Path,
    output_path: Path,
    *,
    metric: Literal["cosine", "dot"] = "cosine",
) -> None:
    """Attach satellites to existing nucleus clusters."""
    embeddings_dir = Path(embeddings_dir)
    nucleus_vectors, satellite_vectors, index = _load_embeddings(embeddings_dir)
    clusters_payload = _load_clusters(clusters_path)

    clusters = clusters_payload["clusters"]
    cluster_map = {cluster["cluster_id"]: cluster for cluster in clusters}

    nucleus_to_cluster: dict[str, int] = {}
    for cluster in clusters:
        for member in cluster["members"]:
            nucleus_to_cluster[member["edu_id"]] = cluster["cluster_id"]
        cluster.setdefault("satellites", [])

    centroids = np.asarray([cluster["centroid"] for cluster in clusters], dtype=np.float32)
    if metric == "cosine":
        centroids = _normalize_matrix(centroids)
        satellite_vectors = _normalize_matrix(satellite_vectors)

    assignments = []
    for idx, sat_meta in enumerate(index["satellite"]):
        parent_id = sat_meta.get("parent_edu_id")
        if parent_id and parent_id in nucleus_to_cluster:
            cluster_id = nucleus_to_cluster[parent_id]
            attachment = "parent"
            score = None
        else:
            if centroids.size == 0:
                raise ValueError("Cannot attach satellites without clusters.")
            vector = satellite_vectors[idx]
            scores = centroids @ vector
            cluster_id = int(np.argmax(scores))
            attachment = "nearest"
            score = float(scores[cluster_id])

        satellite_record = {
            "index": idx,
            "doc_id": sat_meta["doc_id"],
            "edu_id": sat_meta["edu_id"],
            "topic_id": sat_meta.get("topic_id"),
            "parent_edu_id": parent_id,
            "relation": sat_meta.get("relation"),
            "span": sat_meta.get("span"),
            "attachment": attachment,
            "score": score,
        }

        enriched_record = {**satellite_record, "cluster_id": cluster_id}
        cluster_map[cluster_id]["satellites"].append(enriched_record)
        assignments.append(enriched_record)

    payload = {
        "metadata": {
            "embeddings": str(embeddings_dir.resolve()),
            "clusters": str(Path(clusters_path).resolve()),
            "metric": metric,
        },
        "counts": {
            "clusters": len(clusters),
            "satellites": len(assignments),
        },
        "clusters": clusters,
        "assignments": assignments,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)
