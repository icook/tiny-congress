"""Typer CLI entry points for the RSTFlow pipeline."""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path

import typer

SRC_DIR = Path(__file__).resolve().parent
if str(SRC_DIR) not in sys.path:  # pragma: no cover - runtime convenience
    sys.path.insert(0, str(SRC_DIR))

try:  # pragma: no cover
    from .aggregate import aggregate_clusters
    from .attach_satellites import attach_satellites
    from .cluster_nucleus import cluster_nuclei
    from .embed import embed_edus
    from .flatten_rst import flatten_trees
    from .rst_parse import RSTParser, parse_corpus
    from .synth_data import generate_docs
except ImportError:  # pragma: no cover
    from aggregate import aggregate_clusters  # type: ignore
    from attach_satellites import attach_satellites  # type: ignore
    from cluster_nucleus import cluster_nuclei  # type: ignore
    from embed import embed_edus  # type: ignore
    from flatten_rst import flatten_trees  # type: ignore
    from rst_parse import RSTParser, parse_corpus  # type: ignore
    from synth_data import generate_docs  # type: ignore

PROJECT_ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = PROJECT_ROOT / "data"
RAW_DIR = DATA_DIR / "raw"
RST_DIR = DATA_DIR / "rst"
EDU_DIR = DATA_DIR / "edus"
EMBED_DIR = DATA_DIR / "embeddings"
CLUSTER_DIR = DATA_DIR / "clusters"
SNAPSHOT_DIR = DATA_DIR / "snapshots"

DEFAULT_DOCS = RAW_DIR / "docs.jsonl"
DEFAULT_RST = RST_DIR / "rst_trees.jsonl"
DEFAULT_EDUS = EDU_DIR / "edus.jsonl"
DEFAULT_EMBED = EMBED_DIR
DEFAULT_NUCLEUS_CLUSTERS = CLUSTER_DIR / "nucleus_clusters.json"
DEFAULT_CLUSTER_WITH_SATELLITES = CLUSTER_DIR / "clusters_with_satellites.json"
DEFAULT_SNAPSHOT = SNAPSHOT_DIR / "final_bullets.json"

app = typer.Typer(help="RSTFlow pipeline CLI.")


@dataclass
class SentenceSplitParser:
    """Fallback parser that splits on sentence boundaries."""

    name: str = "sentence_split"
    version: str = "0.1"

    def parse(self, text: str) -> dict:
        sentences = [part.strip() for part in text.split(".") if part.strip()]
        edus = []
        relations = []
        previous: str | None = None
        for index, sentence in enumerate(sentences, start=1):
            edu_id = f"e{index:03d}"
            edus.append({"edu_id": edu_id, "text": sentence})
            if previous:
                relations.append(
                    {
                        "child_id": edu_id,
                        "parent_id": previous,
                        "relation": "sequence",
                        "nuclearity": "satellite",
                    }
                )
            previous = edu_id
        return {"edus": edus, "relations": relations, "root_edu": edus[0]["edu_id"] if edus else None}


def _resolve_parser(backend: str) -> RSTParser:
    if backend == "stub":
        return SentenceSplitParser()
    raise typer.BadParameter(f"Unsupported parser backend '{backend}'.")


@app.command("synth-data")
def synth_data_cli(
    count: int = typer.Option(100, "--count", "-n", help="Number of documents to generate."),
    seed: int = typer.Option(13, "--seed", help="Seed for deterministic generation."),
    output: Path = typer.Option(
        DEFAULT_DOCS,
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


@app.command("rst-parse")
def rst_parse_cli(
    input_path: Path = typer.Option(
        DEFAULT_DOCS,
        "--input",
        "-i",
        exists=True,
        readable=True,
        resolve_path=True,
        help="Input raw documents JSONL.",
    ),
    output_path: Path = typer.Option(
        DEFAULT_RST,
        "--output",
        "-o",
        resolve_path=True,
        help="Output path for RST parse JSONL.",
    ),
    parser_backend: str = typer.Option(
        "stub", "--backend", "-b", help="Parser backend to use (default: stub sentence splitter)."
    ),
) -> None:
    """Run the RST parser and materialise JSONL trees."""
    parser = _resolve_parser(parser_backend)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    parse_corpus(input_path=input_path, output_path=output_path, parser=parser)
    typer.echo(f"Wrote parses to {output_path}")


@app.command("flatten")
def flatten_cli(
    input_path: Path = typer.Option(
        DEFAULT_RST,
        "--input",
        "-i",
        exists=True,
        readable=True,
        resolve_path=True,
        help="RST parse JSONL file.",
    ),
    output_path: Path = typer.Option(
        DEFAULT_EDUS,
        "--output",
        "-o",
        resolve_path=True,
        help="Destination EDU JSONL file.",
    ),
) -> None:
    """Flatten RST trees into EDU rows."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    flatten_trees(input_path=input_path, output_path=output_path)
    typer.echo(f"Wrote flattened EDUs to {output_path}")


@app.command("embed")
def embed_cli(
    input_path: Path = typer.Option(
        DEFAULT_EDUS,
        "--input",
        "-i",
        exists=True,
        readable=True,
        resolve_path=True,
        help="Flattened EDU JSONL.",
    ),
    output_dir: Path = typer.Option(
        DEFAULT_EMBED,
        "--output-dir",
        "-o",
        resolve_path=True,
        help="Directory to write embeddings and index metadata.",
    ),
    model_name: str = typer.Option(
        "sentence-transformers/all-mpnet-base-v2",
        "--model",
        help="SentenceTransformer model to use.",
    ),
    device: str = typer.Option(
        None,
        "--device",
        help="Device for inference (e.g., cpu or cuda). Defaults to auto.",
    ),
) -> None:
    """Generate embeddings for nucleus/satellite EDUs."""
    embed_edus(
        input_path=input_path,
        output_dir=output_dir,
        model_name=model_name,
        device=device,  # type: ignore[arg-type]
    )
    typer.echo(f"Wrote embeddings under {output_dir}")


@app.command("cluster")
def cluster_cli(
    embeddings_dir: Path = typer.Option(
        DEFAULT_EMBED,
        "--embeddings",
        "-e",
        exists=True,
        resolve_path=True,
        help="Embedding directory containing nucleus.npy and index.jsonl.",
    ),
    output_path: Path = typer.Option(
        DEFAULT_NUCLEUS_CLUSTERS,
        "--output",
        "-o",
        resolve_path=True,
        help="Destination JSON file for nucleus clusters.",
    ),
    k: int = typer.Option(
        5,
        "--k",
        min=1,
        help="Number of clusters to form.",
    ),
    seed: int = typer.Option(13, "--seed", help="Random seed for clustering."),
) -> None:
    """Cluster nucleus embeddings."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    cluster_nuclei(
        embeddings_path=embeddings_dir,
        output_path=output_path,
        k=k,
        seed=seed,
    )
    typer.echo(f"Wrote nucleus clusters to {output_path}")


@app.command("attach")
def attach_cli(
    embeddings_dir: Path = typer.Option(
        DEFAULT_EMBED,
        "--embeddings",
        "-e",
        exists=True,
        resolve_path=True,
        help="Embedding directory with nucleus/satellite vectors and index.",
    ),
    clusters_path: Path = typer.Option(
        DEFAULT_NUCLEUS_CLUSTERS,
        "--clusters",
        "-c",
        exists=True,
        resolve_path=True,
        help="Nucleus cluster JSON produced by the cluster stage.",
    ),
    output_path: Path = typer.Option(
        DEFAULT_CLUSTER_WITH_SATELLITES,
        "--output",
        "-o",
        resolve_path=True,
        help="Destination JSON file with clusters and satellite assignments.",
    ),
) -> None:
    """Attach satellites to nucleus clusters."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    attach_satellites(
        embeddings_dir=embeddings_dir,
        clusters_path=clusters_path,
        output_path=output_path,
    )
    typer.echo(f"Wrote cluster assignments to {output_path}")


@app.command("aggregate")
def aggregate_cli(
    flattened_path: Path = typer.Option(
        DEFAULT_EDUS,
        "--flat",
        "-f",
        exists=True,
        resolve_path=True,
        help="Flattened EDU JSONL file.",
    ),
    clusters_with_satellites_path: Path = typer.Option(
        DEFAULT_CLUSTER_WITH_SATELLITES,
        "--clusters",
        "-c",
        exists=True,
        resolve_path=True,
        help="Cluster JSON augmented with satellite assignments.",
    ),
    output_path: Path = typer.Option(
        DEFAULT_SNAPSHOT,
        "--output",
        "-o",
        resolve_path=True,
        help="Destination aggregate snapshot JSON.",
    ),
) -> None:
    """Aggregate clusters into final bullet structure."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    aggregate_clusters(
        flattened_path=flattened_path,
        clusters_with_satellites_path=clusters_with_satellites_path,
        output_path=output_path,
    )
    typer.echo(f"Wrote aggregate snapshot to {output_path}")


def run() -> None:
    """Entrypoint when invoking via `python -m`."""
    app()


if __name__ == "__main__":
    run()
