"""Typer CLI entry points for the RSTFlow pipeline."""

from __future__ import annotations

import sys
from pathlib import Path

import typer

SRC_DIR = Path(__file__).resolve().parent
if str(SRC_DIR) not in sys.path:  # pragma: no cover - runtime convenience
    sys.path.insert(0, str(SRC_DIR))

try:  # pragma: no cover - allow running as script or package
    from .synth_data import generate_docs
except ImportError:  # pragma: no cover
    from synth_data import generate_docs  # type: ignore

PROJECT_ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = PROJECT_ROOT / "data"
RAW_DIR = DATA_DIR / "raw"
DEFAULT_OUTPUT = RAW_DIR / "docs.jsonl"


def synth_data_cli(
    count: int = typer.Option(100, "--count", "-n", help="Number of documents to generate."),
    seed: int = typer.Option(13, "--seed", help="Seed for deterministic generation."),
    output: Path = typer.Option(
        DEFAULT_OUTPUT,
        "--output",
        "-o",
        file_okay=True,
        dir_okay=False,
        writable=True,
        resolve_path=True,
        help="Destination JSONL file.",
    ),
    backend: str = typer.Option(
        "template",
        "--backend",
        "-b",
        help="Synthetic data backend: 'template' or 'lmstudio'.",
        case_sensitive=False,
    ),
    lmstudio_url: str = typer.Option(
        "http://127.0.0.1:1234/v1/chat/completions",
        "--lmstudio-url",
        help="LM Studio OpenAI-compatible completions endpoint.",
    ),
    lmstudio_model: str = typer.Option(
        "openai/gpt-oss-20b",
        "--lmstudio-model",
        help="LM Studio model identifier.",
    ),
    temperature: float = typer.Option(
        0.8,
        "--temperature",
        help="Sampling temperature for LM Studio backend.",
    ),
    timeout: int = typer.Option(
        120,
        "--timeout",
        help="HTTP timeout (seconds) for LM Studio backend.",
    ),
) -> None:
    """Generate synthetic civic-discourse documents."""
    selected_backend = backend.lower()
    generate_docs(
        output_path=output,
        count=count,
        seed=seed,
        backend=selected_backend,
        lmstudio_url=lmstudio_url,
        lmstudio_model=lmstudio_model,
        temperature=temperature,
        timeout=timeout,
    )
    typer.echo(f"Wrote {count} documents to {output}")


def run() -> None:
    """Entrypoint when invoking via `python -m`."""
    typer.run(synth_data_cli)


if __name__ == "__main__":
    run()
