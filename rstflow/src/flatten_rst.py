"""Transform RST trees into flat EDU records."""

from __future__ import annotations

from pathlib import Path

try:  # pragma: no cover - package/script compatibility
    from .io_utils import read_jsonl, write_jsonl
    from .schemas.rst import RSTParseResult
except ImportError:  # pragma: no cover
    from io_utils import read_jsonl, write_jsonl  # type: ignore
    from schemas.rst import RSTParseResult  # type: ignore


def flatten_trees(input_path: Path, output_path: Path) -> None:
    """Flatten parsed RST trees into EDU-level JSONL records."""
    parsed_docs = read_jsonl(input_path)
    rows: list[dict[str, object]] = []

    for item in parsed_docs:
        doc_id = item["doc_id"]
        topic_id = item.get("topic_id")
        parse_result = RSTParseResult.model_validate(item["rst"])

        relation_by_child = {rel.child_id: rel for rel in parse_result.relations}

        for edu in parse_result.edus:
            relation = relation_by_child.get(edu.edu_id)

            if relation:
                nuclearity = relation.nuclearity
                relation_label = relation.relation
                parent_id = relation.parent_id
            else:
                nuclearity = "nucleus"
                relation_label = None
                parent_id = None

            rows.append(
                {
                    "doc_id": doc_id,
                    "topic_id": topic_id,
                    "edu_id": edu.edu_id,
                    "text": edu.text,
                    "span": edu.span,
                    "nuclearity": nuclearity,
                    "relation": relation_label,
                    "parent_edu_id": parent_id,
                    "is_root": parse_result.root_edu == edu.edu_id,
                }
            )

    write_jsonl(output_path, rows)
