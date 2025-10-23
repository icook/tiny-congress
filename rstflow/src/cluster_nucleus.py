"""Cluster nucleus embeddings."""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np
from sklearn.cluster import KMeans

try:  # pragma: no cover
    from .io_utils import read_jsonl
except ImportError:  # pragma: no cover
    from io_utils import read_jsonl  # type: ignore


def _load_embeddings(embeddings_dir: Path) -> tuple[np.ndarray, dict]:
    nucleus_path = embeddings_dir / "nucleus.npy"
    index_path = embeddings_dir / "index.jsonl"

    if not nucleus_path.exists():
        raise FileNotFoundError(f"Missing nucleus embedding file at {nucleus_path}")
    if not index_path.exists():
        raise FileNotFoundError(f"Missing index metadata at {index_path}")

    vectors = np.load(nucleus_path)
    metadata = read_jsonl(index_path)[0]
    return vectors, metadata


def cluster_nuclei(
    embeddings_path: Path,
    output_path: Path,
    k: int,
    seed: int = 13,
) -> None:
    """Cluster nucleus vectors."""
    embeddings_dir = Path(embeddings_path)
    vectors, metadata = _load_embeddings(embeddings_dir)

    if vectors.size == 0:
        raise ValueError("No nucleus embeddings available to cluster.")
    if k <= 0:
        raise ValueError("k must be greater than zero.")
    if k > len(vectors):
        raise ValueError(f"k={k} exceeds number of nuclei ({len(vectors)}).")

    model = KMeans(
        n_clusters=k,
        random_state=seed,
        n_init="auto",
    )
    labels = model.fit_predict(vectors)

    clusters: list[dict] = []
    centroid_vectors = model.cluster_centers_

    for cluster_id in range(k):
        member_indices = np.where(labels == cluster_id)[0]
        members = []
        for idx in member_indices:
            nucleus_meta = metadata["nucleus"][int(idx)]
            members.append(
                {
                    "index": int(idx),
                    "doc_id": nucleus_meta["doc_id"],
                    "edu_id": nucleus_meta["edu_id"],
                    "topic_id": nucleus_meta.get("topic_id"),
                    "relation": nucleus_meta.get("relation"),
                    "span": nucleus_meta.get("span"),
                    "is_root": nucleus_meta.get("is_root", False),
                }
            )

        clusters.append(
            {
                "cluster_id": int(cluster_id),
                "size": int(len(member_indices)),
                "centroid": centroid_vectors[cluster_id].tolist(),
                "members": members,
            }
        )

    payload = {
        "model": {
            "name": "kmeans",
            "params": {
                "n_clusters": k,
                "random_state": seed,
                "n_init": model.n_init,
            },
            "inertia": float(model.inertia_),
        },
        "counts": {
            "clusters": k,
            "nucleus": len(vectors),
        },
        "clusters": clusters,
        "index_path": str((embeddings_dir / "index.jsonl").resolve()),
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)
