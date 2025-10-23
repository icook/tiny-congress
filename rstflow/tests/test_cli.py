"""CLI smoke tests for the Click entrypoints."""

from pathlib import Path

from click.testing import CliRunner

from cli import cli

runner = CliRunner()


def test_cli_root_help_displays() -> None:
    result = runner.invoke(cli, ["--help"])
    assert result.exit_code == 0
    assert "RSTFlow pipeline CLI." in result.stdout


def test_cli_synth_data_generates_jsonl() -> None:
    with runner.isolated_filesystem():
        result = runner.invoke(
            cli,
            ["synth-data", "--count", "3", "--output", "docs.jsonl", "--seed", "5"],
        )
        assert result.exit_code == 0, result.stdout

        output_path = Path("docs.jsonl")
        assert output_path.exists()
        lines = [line for line in output_path.read_text().splitlines() if line.strip()]
        assert len(lines) == 3


def test_full_stub_pipeline_via_cli() -> None:
    with runner.isolated_filesystem():
        docs_path = Path("docs.jsonl")
        rst_path = Path("rst.jsonl")
        edus_path = Path("edus.jsonl")
        embed_dir = Path("embeddings")
        clusters_path = Path("clusters.json")
        assignments_path = Path("clusters_with_satellites.json")
        snapshot_path = Path("snapshot.json")

        assert runner.invoke(
            cli,
            ["synth-data", "--count", "4", "--output", str(docs_path), "--seed", "11"],
        ).exit_code == 0

        assert runner.invoke(
            cli,
            ["rst-parse", "--input", str(docs_path), "--output", str(rst_path), "--backend", "stub"],
        ).exit_code == 0

        assert runner.invoke(
            cli,
            ["flatten", "--input", str(rst_path), "--output", str(edus_path)],
        ).exit_code == 0

        assert runner.invoke(
            cli,
            ["embed", "--input", str(edus_path), "--output-dir", str(embed_dir), "--model", "stub"],
        ).exit_code == 0

        assert runner.invoke(
            cli,
            ["cluster", "--embeddings", str(embed_dir), "--output", str(clusters_path), "--k", "2"],
        ).exit_code == 0

        assert runner.invoke(
            cli,
            [
                "attach",
                "--embeddings",
                str(embed_dir),
                "--clusters",
                str(clusters_path),
                "--output",
                str(assignments_path),
            ],
        ).exit_code == 0

        assert runner.invoke(
            cli,
            [
                "aggregate",
                "--flat",
                str(edus_path),
                "--clusters",
                str(assignments_path),
                "--output",
                str(snapshot_path),
            ],
        ).exit_code == 0

        assert snapshot_path.exists()
        assert "clusters" in snapshot_path.read_text()
