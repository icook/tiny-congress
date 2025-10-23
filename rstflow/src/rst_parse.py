"""RST parsing stage wrappers."""

from __future__ import annotations

from pathlib import Path
from typing import Protocol

from pydantic import ValidationError

try:  # pragma: no cover - allow usage as package or script
    from .io_utils import read_jsonl, write_jsonl
    from .schemas.documents import RawDocument
    from .schemas.rst import RSTParseResult
except ImportError:  # pragma: no cover
    from io_utils import read_jsonl, write_jsonl  # type: ignore
    from schemas.documents import RawDocument  # type: ignore
    from schemas.rst import RSTParseResult  # type: ignore


class RSTParser(Protocol):
    """Protocol defining minimum interface for RST parsers."""

    def parse(self, text: str) -> RSTParseResult | dict:  # pragma: no cover - structural typing
        """Parse raw text and return a structured RST tree."""


def parse_corpus(
    input_path: Path,
    output_path: Path,
    parser: RSTParser,
) -> None:
    """Run the configured parser across all input documents."""
    documents = read_jsonl(input_path)
    rows = []

    parser_name = getattr(parser, "name", parser.__class__.__name__)
    parser_version = getattr(parser, "version", None)

    for raw in documents:
        doc = RawDocument.model_validate(raw)
        try:
            parse_result = RSTParseResult.model_validate(parser.parse(doc.text))
        except ValidationError as exc:  # pragma: no cover - defensive logging
            raise ValueError(f"Parser returned invalid structure for doc {doc.doc_id}") from exc

        rows.append(
            {
                "doc_id": doc.doc_id,
                "topic_id": doc.topic_id,
                "parser": {"name": parser_name, "version": parser_version},
                "rst": parse_result.model_dump(),
            }
        )

    write_jsonl(output_path, rows)
