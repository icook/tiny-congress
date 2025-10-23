"""Click-based CLI entry points for the RSTFlow pipeline."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Optional

import click

try:  # pragma: no cover - allow usage as package or script
    from .aggregate import aggregate_clusters
    from .attach_satellites import attach_satellites
    from .cluster_nucleus import cluster_nuclei
    from .embed import embed_edus
    from .flatten_rst import flatten_trees
    from .rst_parse import RSTParser, parse_corpus
    from .synth_data import generate_docs
    from .parsers import (
        IsaNLPNotInstalledError,
        IsaNLPParserAdapter,
        IsaNLPRuntimeError,
    )
except ImportError:  # pragma: no cover
    from aggregate import aggregate_clusters  # type: ignore
    from attach_satellites import attach_satellites  # type: ignore
    from cluster_nucleus import cluster_nuclei  # type: ignore
    from embed import embed_edus  # type: ignore
    from flatten_rst import flatten_trees  # type: ignore
    from rst_parse import RSTParser, parse_corpus  # type: ignore
    from synth_data import generate_docs  # type: ignore
    try:  # pragma: no cover - when parsers package is unavailable
        from parsers import (  # type: ignore
            IsaNLPNotInstalledError,
            IsaNLPParserAdapter,
            IsaNLPRuntimeError,
        )
    except ImportError:  # pragma: no cover
        IsaNLPParserAdapter = None  # type: ignore
        IsaNLPNotInstalledError = None  # type: ignore
        IsaNLPRuntimeError = None  # type: ignore

if "IsaNLPParserAdapter" not in globals():  # pragma: no cover - safety net
    IsaNLPParserAdapter = None  # type: ignore
    IsaNLPNotInstalledError = None  # type: ignore
    IsaNLPRuntimeError = None  # type: ignore


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


@dataclass
class SentenceSplitParser:
    """Fallback parser that splits text on sentence boundaries."""

    name: str = "sentence_split"
    version: str = "0.1"

    def parse(self, text: str) -> dict:
        sentences = [part.strip() for part in text.split(".") if part.strip()]
        edus = []
        relations = []
        previous: Optional[str] = None

        for index, sentence in enumerate(sentences, start=1):
            edu_id = f"e{index:03d}"
            edus.append({"edu_id": edu_id, "text": sentence})
            if previous is not None:
                relations.append(
                    {
                        "child_id": edu_id,
                        "parent_id": previous,
                        "relation": "sequence",
                        "nuclearity": "satellite",
                    }
                )
            previous = edu_id

        return {
            "edus": edus,
            "relations": relations,
            "root_edu": edus[0]["edu_id"] if edus else None,
        }


def _ensure_parent(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)


def _resolve_parser(
    backend: str,
    *,
    isanlp_model: str,
    isanlp_version: str,
    isanlp_device: int,
    isanlp_relinventory: Optional[str],
) -> RSTParser:
    lowered = backend.lower()
    if lowered == "stub":
        return SentenceSplitParser()
    if lowered == "isanlp":
        if IsaNLPParserAdapter is None:
            raise click.UsageError(
                "isanlp-rst is not installed. Install it via 'pip install isanlp-rst' to use the IsaNLP backend."
            )
        try:
            return IsaNLPParserAdapter(
                model_name=isanlp_model,
                model_version=isanlp_version,
                cuda_device=isanlp_device,
                relinventory=isanlp_relinventory,
            )
        except IsaNLPNotInstalledError as exc:  # pragma: no cover - handled above
            raise click.UsageError(str(exc)) from exc
        except IsaNLPRuntimeError as exc:
            raise click.ClickException(str(exc)) from exc
    raise click.BadParameter(f"Unsupported parser backend '{backend}'.")


@click.group(help="RSTFlow pipeline CLI.")
def cli() -> None:
    """Command group for the pipeline."""


@cli.command("synth-data")
@click.option(
    "--count",
    "-n",
    default=100,
    show_default=True,
    type=click.IntRange(1),
    help="Number of documents to generate.",
)
@click.option(
    "--seed",
    default=13,
    show_default=True,
    type=int,
    help="Seed for deterministic generation.",
)
@click.option(
    "--output",
    "-o",
    default=str(DEFAULT_DOCS),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, writable=True, resolve_path=True),
    help="Destination JSONL file.",
)
@click.option(
    "--backend",
    "-b",
    default="template",
    show_default=True,
    type=click.Choice(["template", "lmstudio"], case_sensitive=False),
    help="Synthetic data backend.",
)
@click.option(
    "--lmstudio-url",
    default="http://127.0.0.1:1234/v1/chat/completions",
    show_default=True,
    help="LM Studio OpenAI-compatible completions endpoint.",
)
@click.option(
    "--lmstudio-model",
    default="openai/gpt-oss-20b",
    show_default=True,
    help="LM Studio model identifier.",
)
@click.option(
    "--temperature",
    default=0.8,
    show_default=True,
    type=float,
    help="Sampling temperature for LM Studio backend.",
)
@click.option(
    "--timeout",
    default=120,
    show_default=True,
    type=click.IntRange(1),
    help="HTTP timeout (seconds) for LM Studio backend.",
)
def synth_data_cli(
    count: int,
    seed: int,
    output: Path,
    backend: str,
    lmstudio_url: str,
    lmstudio_model: str,
    temperature: float,
    timeout: int,
) -> None:
    """Generate synthetic civic-discourse documents."""
    _ensure_parent(output)
    generate_docs(
        output_path=output,
        count=count,
        seed=seed,
        backend=backend.lower(),
        lmstudio_url=lmstudio_url,
        lmstudio_model=lmstudio_model,
        temperature=temperature,
        timeout=timeout,
    )
    click.echo(f"Wrote {count} documents to {output}")


@cli.command("rst-parse")
@click.option(
    "--input",
    "-i",
    default=str(DEFAULT_DOCS),
    show_default=True,
    type=click.Path(path_type=Path, exists=True, dir_okay=False, readable=True, resolve_path=True),
    help="Input raw documents JSONL.",
)
@click.option(
    "--output",
    "-o",
    default=str(DEFAULT_RST),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, writable=True, resolve_path=True),
    help="Output path for RST parse JSONL.",
)
@click.option(
    "--backend",
    "-b",
    default="stub",
    show_default=True,
    type=click.Choice(["stub", "isanlp"], case_sensitive=False),
    help="Parser backend to use (default: stub sentence splitter).",
)
@click.option(
    "--isanlp-model",
    default="tchewik/isanlp_rst_v3",
    show_default=True,
    help="IsaNLP Hugging Face model identifier.",
)
@click.option(
    "--isanlp-version",
    default="rstdt",
    show_default=True,
    help="IsaNLP model version (hf_model_version).",
)
@click.option(
    "--isanlp-device",
    default=-1,
    show_default=True,
    type=int,
    help="CUDA device index (-1 for CPU).",
)
@click.option(
    "--isanlp-relinventory",
    default=None,
    help="Optional IsaNLP relation inventory override.",
)
def rst_parse_cli(
    input: Path,  # noqa: A002
    output: Path,
    backend: str,
    isanlp_model: str,
    isanlp_version: str,
    isanlp_device: int,
    isanlp_relinventory: Optional[str],
) -> None:
    """Run the RST parser and materialise JSONL trees."""
    parser = _resolve_parser(
        backend,
        isanlp_model=isanlp_model,
        isanlp_version=isanlp_version,
        isanlp_device=isanlp_device,
        isanlp_relinventory=isanlp_relinventory,
    )
    _ensure_parent(output)
    parse_corpus(input_path=input, output_path=output, parser=parser)
    click.echo(f"Wrote parses to {output}")


@cli.command("flatten")
@click.option(
    "--input",
    "-i",
    default=str(DEFAULT_RST),
    show_default=True,
    type=click.Path(path_type=Path, exists=True, dir_okay=False, readable=True, resolve_path=True),
    help="RST parse JSONL file.",
)
@click.option(
    "--output",
    "-o",
    default=str(DEFAULT_EDUS),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, writable=True, resolve_path=True),
    help="Destination EDU JSONL file.",
)
def flatten_cli(
    input: Path,  # noqa: A002
    output: Path,
) -> None:
    """Flatten RST trees into EDU rows."""
    _ensure_parent(output)
    flatten_trees(input_path=input, output_path=output)
    click.echo(f"Wrote flattened EDUs to {output}")


@cli.command("embed")
@click.option(
    "--input",
    "-i",
    default=str(DEFAULT_EDUS),
    show_default=True,
    type=click.Path(path_type=Path, exists=True, dir_okay=False, readable=True, resolve_path=True),
    help="Flattened EDU JSONL.",
)
@click.option(
    "--output-dir",
    "-o",
    "output_dir",
    default=str(DEFAULT_EMBED),
    show_default=True,
    type=click.Path(path_type=Path, file_okay=False, resolve_path=True),
    help="Directory to write embeddings and index metadata.",
)
@click.option(
    "--model",
    default="sentence-transformers/all-mpnet-base-v2",
    show_default=True,
    help="SentenceTransformer model to use. Use 'stub' for deterministic fallback.",
)
@click.option(
    "--device",
    default=None,
    help="Device for inference (e.g., 'cpu' or 'cuda'). Defaults to auto detection.",
)
def embed_cli(
    input: Path,  # noqa: A002
    output_dir: Path,
    model: str,
    device: Optional[str],
) -> None:
    """Generate embeddings for nucleus and satellite EDUs."""
    output_dir.mkdir(parents=True, exist_ok=True)
    embed_edus(
        input_path=input,
        output_dir=output_dir,
        model_name=model,
        device=device,
    )
    click.echo(f"Wrote embeddings under {output_dir}")


@cli.command("cluster")
@click.option(
    "--embeddings",
    "-e",
    default=str(DEFAULT_EMBED),
    show_default=True,
    type=click.Path(path_type=Path, file_okay=False, exists=True, resolve_path=True),
    help="Embedding directory containing nucleus.npy and index.jsonl.",
)
@click.option(
    "--output",
    "-o",
    default=str(DEFAULT_NUCLEUS_CLUSTERS),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, writable=True, resolve_path=True),
    help="Destination JSON file for nucleus clusters.",
)
@click.option(
    "--k",
    default=5,
    show_default=True,
    type=click.IntRange(1),
    help="Number of clusters to form.",
)
@click.option(
    "--seed",
    default=13,
    show_default=True,
    type=int,
    help="Random seed for clustering.",
)
def cluster_cli(
    embeddings: Path,
    output: Path,
    k: int,
    seed: int,
) -> None:
    """Cluster nucleus embeddings."""
    _ensure_parent(output)
    cluster_nuclei(
        embeddings_path=embeddings,
        output_path=output,
        k=k,
        seed=seed,
    )
    click.echo(f"Wrote nucleus clusters to {output}")


@cli.command("attach")
@click.option(
    "--embeddings",
    "-e",
    default=str(DEFAULT_EMBED),
    show_default=True,
    type=click.Path(path_type=Path, file_okay=False, exists=True, resolve_path=True),
    help="Embedding directory with nucleus/satellite vectors and index.",
)
@click.option(
    "--clusters",
    "-c",
    default=str(DEFAULT_NUCLEUS_CLUSTERS),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, exists=True, resolve_path=True),
    help="Nucleus cluster JSON produced by the cluster stage.",
)
@click.option(
    "--output",
    "-o",
    default=str(DEFAULT_CLUSTER_WITH_SATELLITES),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, writable=True, resolve_path=True),
    help="Destination JSON file with clusters and satellite assignments.",
)
@click.option(
    "--metric",
    default="cosine",
    show_default=True,
    type=click.Choice(["cosine", "dot"], case_sensitive=False),
    help="Similarity metric for nearest-cluster fallback.",
)
def attach_cli(
    embeddings: Path,
    clusters: Path,
    output: Path,
    metric: str,
) -> None:
    """Attach satellites to nucleus clusters."""
    _ensure_parent(output)
    attach_satellites(
        embeddings_dir=embeddings,
        clusters_path=clusters,
        output_path=output,
        metric=metric.lower(),  # type: ignore[arg-type]
    )
    click.echo(f"Wrote cluster assignments to {output}")


@cli.command("aggregate")
@click.option(
    "--flat",
    "-f",
    default=str(DEFAULT_EDUS),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, exists=True, resolve_path=True),
    help="Flattened EDU JSONL file.",
)
@click.option(
    "--clusters",
    "-c",
    default=str(DEFAULT_CLUSTER_WITH_SATELLITES),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, exists=True, resolve_path=True),
    help="Cluster JSON augmented with satellite assignments.",
)
@click.option(
    "--output",
    "-o",
    default=str(DEFAULT_SNAPSHOT),
    show_default=True,
    type=click.Path(path_type=Path, dir_okay=False, writable=True, resolve_path=True),
    help="Destination aggregate snapshot JSON.",
)
def aggregate_cli(
    flat: Path,
    clusters: Path,
    output: Path,
) -> None:
    """Aggregate clusters into final bullet structure."""
    _ensure_parent(output)
    aggregate_clusters(
        flattened_path=flat,
        clusters_with_satellites_path=clusters,
        output_path=output,
    )
    click.echo(f"Wrote aggregate snapshot to {output}")


if __name__ == "__main__":
    cli()
