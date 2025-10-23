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
) -> None:
    """Generate synthetic civic-discourse documents."""
    generate_docs(output_path=output, count=count, seed=seed)
    typer.echo(f"Wrote {count} documents to {output}")


def run() -> None:
    """Entrypoint when invoking via `python -m`."""
    typer.run(synth_data_cli)


if __name__ == "__main__":
    run()
