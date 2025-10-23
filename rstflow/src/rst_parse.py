"""RST parsing stage wrappers."""

from pathlib import Path
from typing import Any, Protocol


class RSTParser(Protocol):
    """Protocol defining minimum interface for RST parsers."""

    def parse(self, text: str) -> Any:
        """Parse raw text and return a structured RST tree."""


def parse_corpus(
    input_path: Path,
    output_path: Path,
    parser: RSTParser,
) -> None:
    """Run the configured parser across all input documents."""
    raise NotImplementedError
