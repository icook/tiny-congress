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
`rst-parse`|(Planned) Run an RST parser and serialise tree output.|`data/rst/rst_trees.jsonl`
`flatten`|(Planned) Flatten RST trees into EDU-level rows.|`data/edus/edus.jsonl`
`embed`|(Planned) Encode EDU nuclei/satellites into dense vectors.|`data/embeddings/*`
`cluster`|(Planned) Cluster nucleus vectors.|`data/clusters/nucleus_clusters.json`
`attach`|(Planned) Attach satellites to clusters.|`data/clusters/cluster_members/*.json`
`aggregate`|(Planned) Build human-readable bullets.|`data/snapshots/final_bullets.json`

Each stage will be testable in isolation; see `tests/` for current coverage.
