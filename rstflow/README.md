# RSTFlow

Prototype pipeline for building RST-aware nucleus/satellite clusters.

## Quick start

```bash
make synth       # generate sample synthetic dataset
make test        # run unit tests
# or run the CLI directly:
python src/cli.py --help
```

The pipeline is organised as explicit stages that materialise JSON/JSONL artifacts under `data/`.

Stage|Description|Artifact
---|---|---
`synth-data`|Generate synthetic civic-discourse documents.|`data/raw/docs.jsonl`
`rst-parse`|Run a configured RST parser and materialise tree output.|`data/rst/rst_trees.jsonl`
`flatten`|Flatten RST trees into EDU-level rows.|`data/edus/edus.jsonl`
`embed`|(Planned) Encode EDU nuclei/satellites into dense vectors.|`data/embeddings/*`
`cluster`|(Planned) Cluster nucleus vectors.|`data/clusters/nucleus_clusters.json`
`attach`|(Planned) Attach satellites to clusters.|`data/clusters/cluster_members/*.json`
`aggregate`|(Planned) Build human-readable bullets.|`data/snapshots/final_bullets.json`

Each stage is designed to be testable in isolation; see `tests/` for current coverage.

## RST parsing quickstart

`rst_parse.parse_corpus` expects a parser object with a `.parse(text)` method that returns data matching the `RSTParseResult` schema. This keeps the file-first pipeline flexible—you can swap in IsaNLP, DMRST, or a stub implementation for unit tests. The flatten stage (`flatten_rst.flatten_trees`) consumes the resulting JSONL and produces one row per EDU with nuclearity, relation, and optional span metadata.
