# RSTFlow

Prototype pipeline for building RST-aware nucleus/satellite clusters.

## Quick start

```bash
make synth       # generate sample synthetic dataset
make test        # run unit tests
# or run the CLI directly:
python src/cli.py --help
```

> Requires Python 3.12+. The provided `Makefile` targets `python3.12` when creating virtual environments.

The pipeline is organised as explicit stages that materialise JSON/JSONL artifacts under `data/`.

Stage|Description|Artifact
---|---|---
`synth-data`|Generate synthetic civic-discourse documents.|`data/raw/docs.jsonl`
`rst-parse`|Run a configured RST parser and materialise tree output.|`data/rst/rst_trees.jsonl`
`flatten`|Flatten RST trees into EDU-level rows.|`data/edus/edus.jsonl`
`embed`|Encode EDU nuclei/satellites into dense vectors.|`data/embeddings/`
`cluster`|Cluster nucleus vectors.|`data/clusters/nucleus_clusters.json`
`attach`|Attach satellites to clusters.|`data/clusters/clusters_with_satellites.json`
`aggregate`|Build human-readable bullets.|`data/snapshots/final_bullets.json`

Each stage is designed to be testable in isolation; see `tests/` for current coverage.

## CLI commands

The Typer CLI exposes one command per stage:

- `python src/cli.py synth-data` — Scaffold synthetic documents (template or LM Studio backends).
- `python src/cli.py rst-parse` — Run a parser backend (`stub` splitter included) across documents.
- `python src/cli.py flatten` — Flatten RST trees into EDU rows.
- `python src/cli.py embed` — Generate embeddings (defaults to `all-mpnet-base-v2`).
- `python src/cli.py cluster` — Cluster nucleus embeddings with KMeans.
- `python src/cli.py attach` — Attach satellite vectors to nucleus clusters.
- `python src/cli.py aggregate` — Build the final aggregated snapshot.

Chain the commands manually or script them for end-to-end execution; each stage reads/writes the artifacts listed above.

## RST parsing quickstart

`rst_parse.parse_corpus` expects a parser object with a `.parse(text)` method that returns data matching the `RSTParseResult` schema. This keeps the file-first pipeline flexible—you can swap in IsaNLP, DMRST, or a stub implementation for unit tests. The flatten stage (`flatten_rst.flatten_trees`) consumes the resulting JSONL and produces one row per EDU with nuclearity, relation, and optional span metadata.
