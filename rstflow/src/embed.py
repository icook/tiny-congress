"""Embed EDU texts using sentence transformers."""

from pathlib import Path
from typing import Literal


def embed_edus(
    input_path: Path,
    output_dir: Path,
    model_name: str = "sentence-transformers/all-mpnet-base-v2",
    device: Literal["cpu", "cuda"] | None = None,
) -> None:
    """Generate embeddings for EDU segments."""
    raise NotImplementedError
