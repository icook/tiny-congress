"""Embed EDU texts using sentence transformers."""

from __future__ import annotations

from pathlib import Path
from typing import Literal, Protocol

import numpy as np

try:  # pragma: no cover - allow module usage directly
    from .io_utils import read_jsonl, write_jsonl
except ImportError:  # pragma: no cover
    from io_utils import read_jsonl, write_jsonl  # type: ignore


class Embedder(Protocol):
    """Protocol for embedding models used in tests and runtime."""

    def encode(self, sentences: list[str], **kwargs) -> np.ndarray:
        """Return embeddings for the supplied sentences."""


def _ensure_embedder(
    embedder: Embedder | None,
    model_name: str,
    device: Literal["cpu", "cuda"] | None,
) -> Embedder:
    if embedder is not None:
        return embedder

    from sentence_transformers import SentenceTransformer  # pragma: no cover

    return SentenceTransformer(model_name, device=device)


def _encode_texts(embedder: Embedder, texts: list[str]) -> np.ndarray:
    if not texts:
        return np.zeros((0, 0), dtype=np.float32)

    vectors = embedder.encode(
        texts,
        convert_to_numpy=True,
        show_progress_bar=False,
        normalize_embeddings=True,
    )
    if isinstance(vectors, list):
        vectors = np.asarray(vectors, dtype=np.float32)
    return vectors


def embed_edus(
    input_path: Path,
    output_dir: Path,
    model_name: str = "sentence-transformers/all-mpnet-base-v2",
    device: Literal["cpu", "cuda"] | None = None,
    *,
    embedder: Embedder | None = None,
) -> None:
    """Generate embeddings for EDU segments."""
    rows = read_jsonl(input_path)
    output_dir.mkdir(parents=True, exist_ok=True)

    nuclei = [row for row in rows if row.get("nuclearity") == "nucleus"]
    satellites = [row for row in rows if row.get("nuclearity") == "satellite"]

    resolved_embedder = _ensure_embedder(embedder, model_name, device)

    nucleus_vectors = _encode_texts(resolved_embedder, [row["text"] for row in nuclei])
    satellite_vectors = _encode_texts(resolved_embedder, [row["text"] for row in satellites])

    if nucleus_vectors.size and satellite_vectors.size:
        dimension = nucleus_vectors.shape[1]
    else:
        dimension = nucleus_vectors.shape[1] if nucleus_vectors.size else satellite_vectors.shape[1] if satellite_vectors.size else 0

    np.save(output_dir / "nucleus.npy", nucleus_vectors)
    np.save(output_dir / "satellite.npy", satellite_vectors)

    index_payload = {
        "model": model_name,
        "device": device,
        "dimension": dimension,
        "counts": {
            "nucleus": len(nuclei),
            "satellite": len(satellites),
        },
        "nucleus": [
            {
                "doc_id": row["doc_id"],
                "edu_id": row["edu_id"],
                "topic_id": row.get("topic_id"),
                "relation": row.get("relation"),
                "span": row.get("span"),
                "is_root": row.get("is_root", False),
            }
            for row in nuclei
        ],
        "satellite": [
            {
                "doc_id": row["doc_id"],
                "edu_id": row["edu_id"],
                "topic_id": row.get("topic_id"),
                "parent_edu_id": row.get("parent_edu_id"),
                "relation": row.get("relation"),
                "span": row.get("span"),
            }
            for row in satellites
        ],
    }

    write_jsonl(output_dir / "index.jsonl", [index_payload])
