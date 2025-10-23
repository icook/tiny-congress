"""Tests for RST parsing stage."""

from pathlib import Path

from io_utils import read_jsonl, write_jsonl
from rst_parse import parse_corpus
from schemas.rst import RSTParseResult


class StubRSTParser:
    """Simple parser that splits text into sentence-level EDUs."""

    name = "stub"
    version = "0.1"

    def parse(self, text: str) -> dict:
        sentences = [part.strip() for part in text.split(".") if part.strip()]
        edus = []
        relations = []
        previous_id: str | None = None

        for index, sentence in enumerate(sentences, start=1):
            edu_id = f"e{index:02d}"
            edus.append({"edu_id": edu_id, "text": sentence})
            if previous_id is not None:
                relations.append(
                    {
                        "child_id": edu_id,
                        "parent_id": previous_id,
                        "relation": "sequence",
                        "nuclearity": "satellite",
                    }
                )
            previous_id = edu_id

        return {
            "edus": edus,
            "relations": relations,
            "root_edu": "e01" if edus else None,
        }


def test_parse_corpus_normalises_parser_output(tmp_path: Path) -> None:
    input_path = tmp_path / "docs.jsonl"
    output_path = tmp_path / "rst.jsonl"
    write_jsonl(
        input_path,
        [
            {
                "doc_id": "d000001",
                "topic_id": "topic.parking-ban-main-st",
                "author_id": "u_0001",
                "text": "First sentence. Second sentence.",
            },
            {
                "doc_id": "d000002",
                "topic_id": "topic.bike-boulevard-elm",
                "author_id": "u_0002",
                "text": "Only sentence here.",
            },
        ],
    )

    parse_corpus(input_path=input_path, output_path=output_path, parser=StubRSTParser())

    results = read_jsonl(output_path)
    assert len(results) == 2

    for row in results:
        assert row["parser"]["name"] == "stub"
        parsed = RSTParseResult.model_validate(row["rst"])
        assert len(parsed.edus) >= 1
        assert parsed.root_edu == parsed.edus[0].edu_id
