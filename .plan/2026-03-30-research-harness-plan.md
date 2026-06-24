# Research Technique Experiment Harness — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Python harness that benchmarks multiple research techniques against seed questions using free/subscription-only infrastructure (SearXNG, ArchiveBox, Claude CLI).

**Architecture:** Standalone Python project in `research/` at repo root. Three primitives (search, scrape, LLM) compose into technique functions. A harness runner dispatches techniques against seed questions, captures structured output, and runs LLM-as-judge evaluation. All LLM calls use `claude -p` (CLI oneshot). No API keys required.

**Tech Stack:** Python 3.12, httpx (SearXNG API), subprocess (ArchiveBox CLI, Claude CLI), dataclasses, JSON I/O

**Design doc:** `.plan/2026-03-28-research-harness-design.md`

---

## Task 1: Project scaffolding

**Files:**
- Create: `research/pyproject.toml`
- Create: `research/README.md`
- Create: `research/config.py`
- Create: `research/primitives/__init__.py`
- Create: `research/techniques/__init__.py`
- Create: `research/eval/__init__.py`
- Create: `research/seeds/questions.json`
- Create: `research/domains.json`
- Create: `research/results/.gitkeep`

**Step 1: Create directory structure**

```bash
mkdir -p research/{primitives,techniques,eval,seeds,results}
touch research/primitives/__init__.py research/techniques/__init__.py research/eval/__init__.py
touch research/results/.gitkeep
```

**Step 2: Write `pyproject.toml`**

```toml
[project]
name = "tc-research-harness"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "httpx>=0.27",
]

[project.optional-dependencies]
dev = ["pytest>=8.0", "pytest-asyncio>=0.23"]
```

**Step 3: Write `config.py`**

Configuration from environment variables with sensible defaults:

```python
"""Harness configuration — all external service URLs from env vars."""

import os
from dataclasses import dataclass, field


@dataclass(frozen=True)
class Config:
    searxng_url: str = field(
        default_factory=lambda: os.environ.get("SEARXNG_URL", "http://localhost:8888")
    )
    archivebox_data_dir: str = field(
        default_factory=lambda: os.environ.get("ARCHIVEBOX_DATA_DIR", "/opt/archivebox/data")
    )
    archivebox_bin: str = field(
        default_factory=lambda: os.environ.get("ARCHIVEBOX_BIN", "archivebox")
    )
    claude_bin: str = field(
        default_factory=lambda: os.environ.get("CLAUDE_BIN", "claude")
    )
    claude_model: str = field(
        default_factory=lambda: os.environ.get("CLAUDE_MODEL", "sonnet")
    )
    results_dir: str = field(
        default_factory=lambda: os.environ.get(
            "RESULTS_DIR",
            os.path.join(os.path.dirname(__file__), "results"),
        )
    )
    domains_file: str = field(
        default_factory=lambda: os.environ.get(
            "DOMAINS_FILE",
            os.path.join(os.path.dirname(__file__), "domains.json"),
        )
    )
```

**Step 4: Write `seeds/questions.json`**

```json
[
    {
        "id": "q1_politics_24h",
        "question": "What happened in US politics in the last 24 hours?",
        "type": "current_events",
        "fresh": true
    },
    {
        "id": "q2_rcv_arguments",
        "question": "What are the arguments for and against ranked-choice voting?",
        "type": "policy",
        "fresh": false
    },
    {
        "id": "q3_congress_legislation",
        "question": "What legislation is currently moving through the US Congress?",
        "type": "factual",
        "fresh": true
    },
    {
        "id": "q4_policy_criticism",
        "question": "What are the main criticisms of recent US immigration policy changes?",
        "type": "adversarial",
        "fresh": true
    },
    {
        "id": "q5_local_news",
        "question": "Summarize the last week of local news in Portland, Oregon.",
        "type": "narrow_scope",
        "fresh": true
    }
]
```

**Step 5: Write `domains.json`**

```json
{
    "government": [
        "congress.gov", "whitehouse.gov", "senate.gov", "house.gov",
        "gao.gov", "cbo.gov", "federalregister.gov",
        "supremecourt.gov", "uscourts.gov"
    ],
    "wire_services": [
        "apnews.com", "reuters.com"
    ],
    "public_media": [
        "npr.org", "pbs.org", "bbc.com", "bbc.co.uk",
        "aljazeera.com"
    ],
    "nonprofit_investigative": [
        "propublica.org", "theintercept.com", "themarkup.org",
        "bellingcat.com", "marshallproject.org"
    ],
    "open_platforms": [
        "wikipedia.org", "en.wikipedia.org",
        "arstechnica.com", "eff.org", "techdirt.com",
        "theguardian.com"
    ],
    "substack": [
        "substack.com"
    ]
}
```

**Step 6: Write `README.md`**

```markdown
# Research Technique Experiment Harness

Benchmarks research techniques against seed questions using free infrastructure.

## Prerequisites

- SearXNG instance (set `SEARXNG_URL`, default `http://localhost:8888`)
- ArchiveBox instance (set `ARCHIVEBOX_DATA_DIR`, default `/opt/archivebox/data`)
- Claude Code CLI (`claude` on PATH)

## Usage

# Install dependencies
cd research && pip install -e ".[dev]"

# Run a single technique on a single question
python -m harness --technique breadth_first --question q1_politics_24h

# Run all techniques on all questions
python -m harness --all

# Run evaluation on existing results
python -m eval.judge results/

## Configuration

All config via environment variables. See `config.py` for defaults.
```

**Step 7: Commit**

```bash
git add research/
git commit -m "feat(research): scaffold experiment harness project"
```

---

## Task 2: LLM primitive — Claude CLI wrapper

**Files:**
- Create: `research/primitives/llm.py`
- Create: `research/tests/test_llm.py`

**Step 1: Write test**

```python
"""Tests for the Claude CLI LLM wrapper."""

import json
from unittest.mock import patch, MagicMock
from research.primitives.llm import claude_oneshot, parse_json_response


def test_parse_json_response_extracts_from_markdown_fence():
    raw = '```json\n{"answer": "hello"}\n```'
    assert parse_json_response(raw) == {"answer": "hello"}


def test_parse_json_response_handles_plain_json():
    raw = '{"answer": "hello"}'
    assert parse_json_response(raw) == {"answer": "hello"}


def test_parse_json_response_returns_none_on_garbage():
    assert parse_json_response("not json at all") is None


def test_claude_oneshot_builds_correct_command():
    with patch("subprocess.run") as mock_run:
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='{"result": "test output"}',
            stderr="",
        )
        result = claude_oneshot("test prompt", model="haiku")

        cmd = mock_run.call_args[0][0]
        assert "claude" in cmd[0] or cmd[0].endswith("claude")
        assert "-p" in cmd
        assert "--model" in cmd
        assert "haiku" in cmd
        assert "--output-format" in cmd
        assert "json" in cmd
```

**Step 2: Run test to verify it fails**

Run: `cd research && python -m pytest tests/test_llm.py -v`
Expected: FAIL — module not found

**Step 3: Write `primitives/llm.py`**

```python
"""Claude Code CLI wrapper — all LLM calls go through here."""

import json
import re
import subprocess
import time
from dataclasses import dataclass
from typing import Any

from research.config import Config


@dataclass
class LLMResult:
    """Result from a single Claude CLI call."""
    content: str
    parsed: dict[str, Any] | None
    wall_clock_seconds: float
    model: str
    raw_stdout: str
    raw_stderr: str


def parse_json_response(raw: str) -> dict[str, Any] | None:
    """Extract JSON from a Claude response, handling markdown fences."""
    # Try direct parse first
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, TypeError):
        pass

    # Try extracting from markdown code fence
    match = re.search(r"```(?:json)?\s*\n(.*?)\n```", raw, re.DOTALL)
    if match:
        try:
            return json.loads(match.group(1))
        except json.JSONDecodeError:
            pass

    return None


def claude_oneshot(
    prompt: str,
    *,
    config: Config | None = None,
    model: str | None = None,
    system_prompt: str | None = None,
    allowed_tools: str | None = None,
    output_format: str = "json",
) -> LLMResult:
    """Run a single Claude CLI oneshot and return the result.

    Args:
        prompt: The user prompt to send.
        config: Harness config (defaults to Config()).
        model: Model override (e.g., "haiku", "sonnet", "opus").
        system_prompt: Optional system prompt.
        allowed_tools: Tool spec string. Empty string "" disables all tools.
                       None uses default (all tools disabled for oneshot).
        output_format: "json", "text", or "stream-json".
    """
    cfg = config or Config()
    effective_model = model or cfg.claude_model

    cmd = [
        cfg.claude_bin,
        "-p", prompt,
        "--output-format", output_format,
        "--model", effective_model,
    ]

    if system_prompt:
        cmd.extend(["--system-prompt", system_prompt])

    if allowed_tools is not None:
        cmd.extend(["--allowed-tools", allowed_tools])
    else:
        # Default: no tools for oneshot synthesis/analysis
        cmd.extend(["--allowed-tools", ""])

    start = time.monotonic()
    proc = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=300,  # 5 min max per call
    )
    elapsed = time.monotonic() - start

    if proc.returncode != 0:
        raise RuntimeError(
            f"claude exited {proc.returncode}: {proc.stderr[:500]}"
        )

    # When output-format is json, Claude wraps in a JSON envelope
    # with a "result" field containing the actual text
    content = proc.stdout
    try:
        envelope = json.loads(proc.stdout)
        if isinstance(envelope, dict) and "result" in envelope:
            content = envelope["result"]
    except (json.JSONDecodeError, TypeError):
        pass

    return LLMResult(
        content=content,
        parsed=parse_json_response(content),
        wall_clock_seconds=elapsed,
        model=effective_model,
        raw_stdout=proc.stdout,
        raw_stderr=proc.stderr,
    )
```

**Step 4: Run tests**

Run: `cd research && python -m pytest tests/test_llm.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add research/primitives/llm.py research/tests/test_llm.py
git commit -m "feat(research): claude CLI oneshot wrapper with JSON parsing"
```

---

## Task 3: Search primitive — SearXNG wrapper

**Files:**
- Create: `research/primitives/search.py`
- Create: `research/tests/test_search.py`

**Step 1: Write test**

```python
"""Tests for the SearXNG search wrapper."""

import json
from unittest.mock import AsyncMock, patch, MagicMock
from research.primitives.search import filter_by_allowed_domains, SearchResult


def test_filter_by_allowed_domains_keeps_matching():
    results = [
        SearchResult(url="https://apnews.com/article/foo", title="AP", snippet="..."),
        SearchResult(url="https://paywalled.com/bar", title="Bad", snippet="..."),
        SearchResult(url="https://npr.org/story", title="NPR", snippet="..."),
    ]
    allowed = ["apnews.com", "npr.org"]
    filtered = filter_by_allowed_domains(results, allowed)
    assert len(filtered) == 2
    assert all(r.url.startswith(("https://apnews", "https://npr")) for r in filtered)


def test_filter_by_allowed_domains_handles_subdomains():
    results = [
        SearchResult(url="https://en.wikipedia.org/wiki/Test", title="Wiki", snippet="..."),
    ]
    allowed = ["wikipedia.org"]
    filtered = filter_by_allowed_domains(results, allowed)
    assert len(filtered) == 1
```

**Step 2: Run test — expect FAIL**

Run: `cd research && python -m pytest tests/test_search.py -v`

**Step 3: Write `primitives/search.py`**

```python
"""SearXNG search client — free, self-hosted web search."""

import json
import os
from dataclasses import dataclass
from urllib.parse import urlparse

import httpx

from research.config import Config


@dataclass
class SearchResult:
    url: str
    title: str
    snippet: str


def filter_by_allowed_domains(
    results: list[SearchResult],
    allowed_domains: list[str],
) -> list[SearchResult]:
    """Filter search results to only allowed domains (including subdomains)."""
    def matches(url: str) -> bool:
        hostname = urlparse(url).hostname or ""
        return any(
            hostname == domain or hostname.endswith(f".{domain}")
            for domain in allowed_domains
        )
    return [r for r in results if matches(r.url)]


def load_allowed_domains(config: Config | None = None) -> list[str]:
    """Load the flat list of all allowed domains from domains.json."""
    cfg = config or Config()
    with open(cfg.domains_file) as f:
        categories = json.load(f)
    return [domain for domains in categories.values() for domain in domains]


def search(
    query: str,
    *,
    config: Config | None = None,
    num_results: int = 20,
    filter_domains: bool = True,
) -> list[SearchResult]:
    """Search via SearXNG JSON API, optionally filtering to allowed domains.

    Args:
        query: Search query string.
        config: Harness config.
        num_results: Max results to request from SearXNG.
        filter_domains: If True, filter results to domains.json allowlist.
    """
    cfg = config or Config()

    resp = httpx.get(
        f"{cfg.searxng_url}/search",
        params={
            "q": query,
            "format": "json",
            "pageno": 1,
            "categories": "general,news",
        },
        timeout=30,
    )
    resp.raise_for_status()
    data = resp.json()

    results = [
        SearchResult(
            url=r["url"],
            title=r.get("title", ""),
            snippet=r.get("content", ""),
        )
        for r in data.get("results", [])[:num_results]
    ]

    if filter_domains:
        allowed = load_allowed_domains(cfg)
        results = filter_by_allowed_domains(results, allowed)

    return results
```

**Step 4: Run tests**

Run: `cd research && python -m pytest tests/test_search.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add research/primitives/search.py research/tests/test_search.py
git commit -m "feat(research): SearXNG search wrapper with domain filtering"
```

---

## Task 4: Scrape primitive — ArchiveBox wrapper

**Files:**
- Create: `research/primitives/scrape.py`
- Create: `research/tests/test_scrape.py`

**Step 1: Write test**

```python
"""Tests for the ArchiveBox scrape wrapper."""

import json
from pathlib import Path
from research.primitives.scrape import find_readability_content


def test_find_readability_content_extracts_text(tmp_path):
    # Simulate ArchiveBox readability output structure
    readability_dir = tmp_path / "archive" / "1234567890" / "readability"
    readability_dir.mkdir(parents=True)
    content = {
        "title": "Test Article",
        "byline": "Author Name",
        "textContent": "This is the extracted article text.",
        "excerpt": "Summary",
    }
    (readability_dir / "content.json").write_text(json.dumps(content))

    result = find_readability_content(readability_dir / "content.json")
    assert result is not None
    assert result.text == "This is the extracted article text."
    assert result.title == "Test Article"


def test_find_readability_content_returns_none_for_missing():
    result = find_readability_content(Path("/nonexistent/path/content.json"))
    assert result is None
```

**Step 2: Run test — expect FAIL**

Run: `cd research && python -m pytest tests/test_scrape.py -v`

**Step 3: Write `primitives/scrape.py`**

```python
"""ArchiveBox scrape client — archive URLs and retrieve extracted text."""

import json
import sqlite3
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from research.config import Config


@dataclass
class ScrapedContent:
    url: str
    title: str
    text: str
    timestamp: str


def find_readability_content(content_json_path: Path) -> ScrapedContent | None:
    """Read a readability content.json file and extract the text."""
    try:
        data = json.loads(content_json_path.read_text())
        return ScrapedContent(
            url=data.get("url", ""),
            title=data.get("title", ""),
            text=data.get("textContent", ""),
            timestamp="",
        )
    except (FileNotFoundError, json.JSONDecodeError, KeyError):
        return None


def archive_urls(
    urls: list[str],
    *,
    config: Config | None = None,
    timeout: int = 120,
) -> None:
    """Submit URLs to ArchiveBox for archiving. Blocks until archiving completes.

    Args:
        urls: List of URLs to archive.
        config: Harness config.
        timeout: Max seconds to wait for archiving.
    """
    if not urls:
        return

    cfg = config or Config()
    url_text = "\n".join(urls)

    subprocess.run(
        [cfg.archivebox_bin, "add", "--parser", "url_list"],
        input=url_text,
        capture_output=True,
        text=True,
        timeout=timeout,
        cwd=cfg.archivebox_data_dir,
    )


def get_archived_content(
    urls: list[str],
    *,
    config: Config | None = None,
) -> dict[str, ScrapedContent | None]:
    """Retrieve extracted text for previously archived URLs.

    Queries ArchiveBox's SQLite index to find snapshot paths,
    then reads readability output from disk.

    Returns:
        Dict mapping URL -> ScrapedContent (or None if not found).
    """
    cfg = config or Config()
    data_dir = Path(cfg.archivebox_data_dir)
    db_path = data_dir / "index.sqlite3"

    if not db_path.exists():
        return {url: None for url in urls}

    results: dict[str, ScrapedContent | None] = {}

    conn = sqlite3.connect(str(db_path))
    try:
        for url in urls:
            row = conn.execute(
                "SELECT timestamp FROM core_snapshot WHERE url = ? LIMIT 1",
                (url,),
            ).fetchone()

            if not row:
                results[url] = None
                continue

            timestamp = row[0]
            readability_path = data_dir / "archive" / timestamp / "readability" / "content.json"
            content = find_readability_content(readability_path)
            if content:
                content.url = url
                content.timestamp = timestamp
            results[url] = content
    finally:
        conn.close()

    return results


def scrape_and_retrieve(
    urls: list[str],
    *,
    config: Config | None = None,
) -> dict[str, ScrapedContent | None]:
    """Archive URLs (if needed) and retrieve extracted text.

    This is the main entry point — combines archiving and retrieval.
    Already-archived URLs are served from cache (ArchiveBox skips re-fetch).
    """
    cfg = config or Config()
    archive_urls(urls, config=cfg)
    return get_archived_content(urls, config=cfg)
```

**Step 4: Run tests**

Run: `cd research && python -m pytest tests/test_scrape.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add research/primitives/scrape.py research/tests/test_scrape.py
git commit -m "feat(research): ArchiveBox scrape wrapper with readability extraction"
```

---

## Task 5: Base technique interface and graph types

**Files:**
- Create: `research/techniques/base.py`
- Create: `research/tests/test_base.py`

**Step 1: Write test**

```python
"""Tests for the base technique types."""

import json
from research.techniques.base import (
    ResearchGraph, Node, Edge, NodeType, NodeStatus, RunMetadata,
)


def test_graph_serializes_to_json():
    graph = ResearchGraph(
        nodes=[
            Node(
                id="n1",
                node_type=NodeType.CLAIM,
                status=NodeStatus.COMPLETE,
                content="Test claim",
                sources=["https://example.com"],
            )
        ],
        edges=[],
    )
    data = graph.to_dict()
    roundtripped = json.loads(json.dumps(data))
    assert roundtripped["nodes"][0]["content"] == "Test claim"
    assert roundtripped["nodes"][0]["node_type"] == "claim"


def test_metadata_tracks_counts():
    meta = RunMetadata()
    meta.record_llm_call(1.5)
    meta.record_llm_call(2.0)
    meta.record_search_query()
    meta.record_url_archived()
    meta.record_url_archived()

    assert meta.llm_calls == 2
    assert meta.search_queries == 1
    assert meta.urls_archived == 2
    assert meta.total_llm_seconds == 3.5
```

**Step 2: Run test — expect FAIL**

Run: `cd research && python -m pytest tests/test_base.py -v`

**Step 3: Write `techniques/base.py`**

```python
"""Base types for research techniques — graph output, metadata, technique interface."""

from __future__ import annotations

import time
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

from research.config import Config
from research.primitives.llm import LLMResult
from research.primitives.search import SearchResult
from research.primitives.scrape import ScrapedContent


class NodeType(str, Enum):
    QUESTION = "question"
    CLAIM = "claim"
    SYNTHESIS = "synthesis"
    BRANCH_POINT = "branch_point"


class NodeStatus(str, Enum):
    PROPOSED = "proposed"
    COMPLETE = "complete"
    DISPUTED = "disputed"


class EdgeType(str, Enum):
    LED_TO = "led_to"
    CONTRADICTS = "contradicts"
    REFINES = "refines"
    SYNTHESIZES = "synthesizes"
    SUPPORTS = "supports"


@dataclass
class Node:
    id: str
    node_type: NodeType
    status: NodeStatus
    content: str
    sources: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "node_type": self.node_type.value,
            "status": self.status.value,
            "content": self.content,
            "sources": self.sources,
            "metadata": self.metadata,
        }


@dataclass
class Edge:
    source_id: str
    target_id: str
    edge_type: EdgeType
    label: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_id": self.source_id,
            "target_id": self.target_id,
            "edge_type": self.edge_type.value,
            "label": self.label,
        }


@dataclass
class ResearchGraph:
    nodes: list[Node] = field(default_factory=list)
    edges: list[Edge] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "nodes": [n.to_dict() for n in self.nodes],
            "edges": [e.to_dict() for e in self.edges],
        }


@dataclass
class RunMetadata:
    """Tracks resource usage during a technique run."""
    llm_calls: int = 0
    search_queries: int = 0
    urls_archived: int = 0
    source_tokens_approx: int = 0
    output_tokens_approx: int = 0
    total_llm_seconds: float = 0.0
    wall_clock_seconds: float = 0.0

    def record_llm_call(self, seconds: float) -> None:
        self.llm_calls += 1
        self.total_llm_seconds += seconds

    def record_search_query(self) -> None:
        self.search_queries += 1

    def record_url_archived(self) -> None:
        self.urls_archived += 1

    def to_dict(self) -> dict[str, Any]:
        return {
            "llm_calls": self.llm_calls,
            "search_queries": self.search_queries,
            "urls_archived": self.urls_archived,
            "source_tokens_approx": self.source_tokens_approx,
            "output_tokens_approx": self.output_tokens_approx,
            "total_llm_seconds": round(self.total_llm_seconds, 2),
            "wall_clock_seconds": round(self.wall_clock_seconds, 2),
        }


class Technique(ABC):
    """Base class for research techniques."""

    name: str

    @abstractmethod
    def run(
        self,
        question: str,
        config: Config,
    ) -> tuple[ResearchGraph, RunMetadata, list[dict[str, Any]]]:
        """Execute the research technique.

        Args:
            question: The research question to investigate.
            config: Harness configuration.

        Returns:
            Tuple of (graph, metadata, raw_sources).
            raw_sources is a list of dicts with url, title, text for each source read.
        """
        ...
```

**Step 4: Run tests**

Run: `cd research && python -m pytest tests/test_base.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add research/techniques/base.py research/tests/test_base.py
git commit -m "feat(research): base technique interface and graph types"
```

---

## Task 6: First technique — breadth-first

This is the simplest technique and validates the full pipeline end-to-end.

**Files:**
- Create: `research/techniques/breadth_first.py`

**Step 1: Write the technique**

```python
"""Breadth-first research: generate queries upfront, scrape all, synthesize once."""

from typing import Any

from research.config import Config
from research.primitives.llm import claude_oneshot
from research.primitives.search import search
from research.primitives.scrape import scrape_and_retrieve
from research.techniques.base import (
    Technique, ResearchGraph, RunMetadata, Node, Edge,
    NodeType, NodeStatus, EdgeType,
)


QUERY_GENERATION_PROMPT = """You are a research assistant. Given a research question, generate 5 diverse search queries that would help answer it comprehensively. Return JSON:

{{"queries": ["query1", "query2", "query3", "query4", "query5"]}}

Research question: {question}"""


SYNTHESIS_PROMPT = """You are a research analyst. Given a research question and source material, produce a structured analysis. Return JSON:

{{
    "claims": [
        {{
            "claim": "A specific factual claim",
            "evidence": "Direct quote or paraphrase from sources",
            "source_urls": ["url1"],
            "confidence": "high|medium|low"
        }}
    ],
    "synthesis": "2-3 paragraph overall synthesis connecting the claims",
    "open_questions": ["Things that remain unclear or need further research"],
    "contradictions": ["Any points where sources disagree"]
}}

Research question: {question}

Source material:
{sources}"""


class BreadthFirst(Technique):
    name = "breadth_first"

    def run(
        self,
        question: str,
        config: Config,
    ) -> tuple[ResearchGraph, RunMetadata, list[dict[str, Any]]]:
        meta = RunMetadata()
        import time
        start = time.monotonic()

        # Step 1: Generate search queries
        query_result = claude_oneshot(
            QUERY_GENERATION_PROMPT.format(question=question),
            config=config,
        )
        meta.record_llm_call(query_result.wall_clock_seconds)
        queries = (query_result.parsed or {}).get("queries", [question])

        # Step 2: Search all queries
        all_results = []
        for q in queries:
            results = search(q, config=config)
            meta.record_search_query()
            all_results.extend(results)

        # Deduplicate by URL
        seen_urls = set()
        unique_results = []
        for r in all_results:
            if r.url not in seen_urls:
                seen_urls.add(r.url)
                unique_results.append(r)

        # Step 3: Scrape all URLs
        urls = [r.url for r in unique_results]
        scraped = scrape_and_retrieve(urls, config=config)
        for url, content in scraped.items():
            if content:
                meta.record_url_archived()

        # Build source text for synthesis
        raw_sources = []
        source_text_parts = []
        for url, content in scraped.items():
            if content and content.text.strip():
                raw_sources.append({
                    "url": url,
                    "title": content.title,
                    "text": content.text[:3000],  # Truncate long articles
                })
                source_text_parts.append(
                    f"### {content.title}\nURL: {url}\n{content.text[:3000]}\n"
                )

        source_text = "\n---\n".join(source_text_parts) if source_text_parts else "(no sources retrieved)"
        meta.source_tokens_approx = len(source_text) // 4  # rough estimate

        # Step 4: Synthesize
        synth_result = claude_oneshot(
            SYNTHESIS_PROMPT.format(question=question, sources=source_text),
            config=config,
        )
        meta.record_llm_call(synth_result.wall_clock_seconds)
        synth = synth_result.parsed or {}

        # Build graph
        graph = ResearchGraph()
        q_node = Node(
            id="q0",
            node_type=NodeType.QUESTION,
            status=NodeStatus.COMPLETE,
            content=question,
        )
        graph.nodes.append(q_node)

        for i, claim_data in enumerate(synth.get("claims", [])):
            claim_node = Node(
                id=f"c{i}",
                node_type=NodeType.CLAIM,
                status=NodeStatus.COMPLETE,
                content=claim_data.get("claim", ""),
                sources=claim_data.get("source_urls", []),
                metadata={
                    "evidence": claim_data.get("evidence", ""),
                    "confidence": claim_data.get("confidence", "unknown"),
                },
            )
            graph.nodes.append(claim_node)
            graph.edges.append(Edge(
                source_id="q0",
                target_id=f"c{i}",
                edge_type=EdgeType.SUPPORTS,
            ))

        if synth.get("synthesis"):
            synth_node = Node(
                id="s0",
                node_type=NodeType.SYNTHESIS,
                status=NodeStatus.COMPLETE,
                content=synth["synthesis"],
                metadata={
                    "open_questions": synth.get("open_questions", []),
                    "contradictions": synth.get("contradictions", []),
                },
            )
            graph.nodes.append(synth_node)
            for i in range(len(synth.get("claims", []))):
                graph.edges.append(Edge(
                    source_id=f"c{i}",
                    target_id="s0",
                    edge_type=EdgeType.SYNTHESIZES,
                ))

        meta.wall_clock_seconds = time.monotonic() - start
        meta.output_tokens_approx = len(synth_result.content) // 4

        return graph, meta, raw_sources
```

**Step 2: Quick smoke test (requires live SearXNG + ArchiveBox)**

Run: `cd research && python -c "from research.techniques.breadth_first import BreadthFirst; print('import ok')"
Expected: no import errors

**Step 3: Commit**

```bash
git add research/techniques/breadth_first.py
git commit -m "feat(research): breadth-first technique — generate queries, scrape all, synthesize"
```

---

## Task 7: Harness runner

**Files:**
- Create: `research/harness.py`
- Create: `research/__main__.py`

**Step 1: Write `harness.py`**

```python
"""Main harness runner — dispatches techniques against seed questions."""

import json
import os
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from research.config import Config
from research.techniques.base import Technique, ResearchGraph, RunMetadata
from research.techniques.breadth_first import BreadthFirst

# Registry of available techniques
TECHNIQUES: dict[str, type[Technique]] = {
    "breadth_first": BreadthFirst,
}


def load_seed_questions(path: str | None = None) -> list[dict[str, Any]]:
    """Load seed questions from JSON file."""
    if path is None:
        path = os.path.join(os.path.dirname(__file__), "seeds", "questions.json")
    with open(path) as f:
        return json.load(f)


def run_single(
    technique_name: str,
    question_id: str,
    *,
    config: Config | None = None,
    questions_path: str | None = None,
) -> dict[str, Any]:
    """Run a single technique on a single question. Returns the result dict."""
    cfg = config or Config()
    questions = load_seed_questions(questions_path)

    question_data = next(
        (q for q in questions if q["id"] == question_id), None
    )
    if not question_data:
        raise ValueError(f"Unknown question ID: {question_id}")

    technique_cls = TECHNIQUES.get(technique_name)
    if not technique_cls:
        raise ValueError(
            f"Unknown technique: {technique_name}. "
            f"Available: {', '.join(TECHNIQUES)}"
        )

    technique = technique_cls()
    timestamp = datetime.now(timezone.utc)
    run_id = f"{timestamp.strftime('%Y%m%d_%H%M%S')}_{technique_name}_{question_id}"

    print(f"Running {technique_name} on '{question_data['question'][:60]}...'")

    graph, meta, raw_sources = technique.run(question_data["question"], cfg)

    result = {
        "run_id": run_id,
        "technique": technique_name,
        "question_id": question_id,
        "question": question_data["question"],
        "question_type": question_data.get("type", "unknown"),
        "timestamp": timestamp.isoformat(),
        "metadata": meta.to_dict(),
        "graph": graph.to_dict(),
        "raw_sources": raw_sources,
        "eval_automated": None,
        "eval_human": None,
    }

    # Save result
    results_dir = Path(cfg.results_dir)
    results_dir.mkdir(parents=True, exist_ok=True)
    result_path = results_dir / f"{run_id}.json"
    with open(result_path, "w") as f:
        json.dump(result, f, indent=2)

    print(f"Saved: {result_path}")
    print(f"  LLM calls: {meta.llm_calls}, searches: {meta.search_queries}, "
          f"URLs: {meta.urls_archived}, time: {meta.wall_clock_seconds:.1f}s")

    return result


def run_all(
    *,
    config: Config | None = None,
    techniques: list[str] | None = None,
    questions: list[str] | None = None,
) -> list[dict[str, Any]]:
    """Run all (or selected) techniques on all (or selected) questions."""
    cfg = config or Config()
    seed_questions = load_seed_questions()

    technique_names = techniques or list(TECHNIQUES.keys())
    question_ids = questions or [q["id"] for q in seed_questions]

    results = []
    total = len(technique_names) * len(question_ids)
    for i, technique_name in enumerate(technique_names):
        for j, question_id in enumerate(question_ids):
            idx = i * len(question_ids) + j + 1
            print(f"\n[{idx}/{total}] ", end="")
            result = run_single(technique_name, question_id, config=cfg)
            results.append(result)

    return results
```

**Step 2: Write `__main__.py`**

```python
"""CLI entry point: python -m research ..."""

import argparse
import sys

from research.config import Config
from research.harness import run_single, run_all, TECHNIQUES, load_seed_questions


def main():
    parser = argparse.ArgumentParser(description="Research technique experiment harness")
    parser.add_argument("--technique", "-t", help="Technique name to run")
    parser.add_argument("--question", "-q", help="Question ID to run")
    parser.add_argument("--all", action="store_true", help="Run all techniques on all questions")
    parser.add_argument("--list", action="store_true", help="List available techniques and questions")

    args = parser.parse_args()
    config = Config()

    if args.list:
        print("Techniques:")
        for name in TECHNIQUES:
            print(f"  {name}")
        print("\nQuestions:")
        for q in load_seed_questions():
            print(f"  {q['id']}: {q['question'][:70]}")
        return

    if args.all:
        run_all(config=config)
    elif args.technique and args.question:
        run_single(args.technique, args.question, config=config)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
```

**Step 3: Verify CLI works**

Run: `cd research && python -m research --list`
Expected: prints techniques and questions

**Step 4: Commit**

```bash
git add research/harness.py research/__main__.py
git commit -m "feat(research): harness runner with CLI — dispatches techniques against seeds"
```

---

## Task 8: LLM-as-judge evaluation

**Files:**
- Create: `research/eval/judge.py`

**Step 1: Write `eval/judge.py`**

```python
"""LLM-as-judge evaluation — scores technique output on 5 criteria."""

import json
from pathlib import Path
from typing import Any

from research.config import Config
from research.primitives.llm import claude_oneshot


JUDGE_PROMPT = """You are an expert research evaluator. Score the following research output on 5 criteria, each 1-5.

## Research Question
{question}

## Research Output (claims and synthesis)
{output}

## Source Material Available
{sources}

## Scoring Criteria

1. **source_diversity** (1-5): Did it pull from multiple independent sources? 1=single source/echo chamber, 5=5+ independent sources with different perspectives.
2. **claim_grounding** (1-5): Are claims actually supported by the cited source text? 1=fabricated or misrepresented, 5=every claim traceable to quoted source.
3. **coverage** (1-5): Did it find the major angles on this topic? 1=missed obvious perspectives, 5=comprehensive including dissent.
4. **structure** (1-5): Does the output decompose into discrete, linkable claims? 1=monolithic blob, 5=clear claim-evidence pairs.
5. **actionability** (1-5): Could a reader make a decision based on this? 1=vague summary, 5=concrete facts, named actors, specific data.

Return JSON:
{{
    "source_diversity": {{"score": N, "reason": "one sentence"}},
    "claim_grounding": {{"score": N, "reason": "one sentence"}},
    "coverage": {{"score": N, "reason": "one sentence"}},
    "structure": {{"score": N, "reason": "one sentence"}},
    "actionability": {{"score": N, "reason": "one sentence"}}
}}"""


def judge_result(
    result: dict[str, Any],
    *,
    config: Config | None = None,
) -> dict[str, Any]:
    """Run LLM-as-judge on a single result. Returns the eval dict."""
    cfg = config or Config()

    # Format output for judge
    graph = result.get("graph", {})
    nodes = graph.get("nodes", [])
    claims_text = "\n".join(
        f"- [{n.get('metadata', {}).get('confidence', '?')}] {n['content']}"
        for n in nodes if n.get("node_type") == "claim"
    )
    synthesis_text = "\n".join(
        n["content"] for n in nodes if n.get("node_type") == "synthesis"
    )
    output_text = f"Claims:\n{claims_text}\n\nSynthesis:\n{synthesis_text}"

    # Format sources for judge
    raw_sources = result.get("raw_sources", [])
    sources_text = "\n---\n".join(
        f"[{s.get('title', 'untitled')}] ({s.get('url', '')})\n{s.get('text', '')[:1000]}"
        for s in raw_sources[:10]  # Cap at 10 sources for judge context
    ) or "(no sources)"

    judge_result = claude_oneshot(
        JUDGE_PROMPT.format(
            question=result["question"],
            output=output_text,
            sources=sources_text,
        ),
        config=cfg,
        system_prompt="You are a strict, fair research evaluator. Be honest about weaknesses.",
    )

    return judge_result.parsed or {}


def judge_results_dir(
    results_dir: str,
    *,
    config: Config | None = None,
) -> None:
    """Run judge on all un-evaluated results in a directory."""
    cfg = config or Config()
    results_path = Path(results_dir)

    for result_file in sorted(results_path.glob("*.json")):
        with open(result_file) as f:
            result = json.load(f)

        if result.get("eval_automated"):
            print(f"Skip (already evaluated): {result_file.name}")
            continue

        print(f"Judging: {result_file.name}...")
        evaluation = judge_result(result, config=cfg)
        result["eval_automated"] = evaluation

        with open(result_file, "w") as f:
            json.dump(result, f, indent=2)

        scores = {k: v.get("score", "?") for k, v in evaluation.items()}
        print(f"  Scores: {scores}")
```

**Step 2: Verify import**

Run: `cd research && python -c "from research.eval.judge import judge_result; print('ok')"`
Expected: ok

**Step 3: Commit**

```bash
git add research/eval/judge.py
git commit -m "feat(research): LLM-as-judge evaluation with 5-criterion rubric"
```

---

## Task 9: Human evaluation helper

**Files:**
- Create: `research/eval/rubric.py`

**Step 1: Write `eval/rubric.py`**

```python
"""Human evaluation helpers — display results side-by-side for comparison."""

import json
from pathlib import Path
from typing import Any


def print_result_summary(result: dict[str, Any]) -> None:
    """Print a single result in a readable format for human evaluation."""
    print(f"\n{'=' * 70}")
    print(f"Technique: {result['technique']}")
    print(f"Question:  {result['question'][:70]}")
    print(f"Time:      {result['metadata']['wall_clock_seconds']:.1f}s")
    print(f"LLM calls: {result['metadata']['llm_calls']}, "
          f"Searches: {result['metadata']['search_queries']}, "
          f"URLs: {result['metadata']['urls_archived']}")

    if result.get("eval_automated"):
        scores = {
            k: v.get("score", "?")
            for k, v in result["eval_automated"].items()
        }
        print(f"Auto scores: {scores}")

    print(f"\n--- Claims ---")
    for node in result.get("graph", {}).get("nodes", []):
        if node.get("node_type") == "claim":
            conf = node.get("metadata", {}).get("confidence", "?")
            print(f"  [{conf}] {node['content']}")

    print(f"\n--- Synthesis ---")
    for node in result.get("graph", {}).get("nodes", []):
        if node.get("node_type") == "synthesis":
            print(f"  {node['content'][:500]}")

    if result.get("eval_human"):
        print(f"\n--- Human Eval ---")
        for k, v in result["eval_human"].items():
            print(f"  {k}: {v}")
    print(f"{'=' * 70}")


def compare_results(results_dir: str, question_id: str) -> None:
    """Print all technique results for a given question side-by-side."""
    results_path = Path(results_dir)
    matches = []

    for result_file in sorted(results_path.glob("*.json")):
        with open(result_file) as f:
            result = json.load(f)
        if result.get("question_id") == question_id:
            matches.append(result)

    if not matches:
        print(f"No results found for question: {question_id}")
        return

    print(f"\nComparing {len(matches)} results for: {matches[0]['question'][:70]}")
    for result in matches:
        print_result_summary(result)


def record_human_eval(result_path: str) -> None:
    """Interactive prompt to record human evaluation scores for a result."""
    with open(result_path) as f:
        result = json.load(f)

    print_result_summary(result)

    print("\n--- Human Evaluation ---")
    print("Score each 1-5 (or press Enter to skip):\n")

    eval_data = {}
    for criterion in ["learned_something", "trust", "would_share"]:
        label = criterion.replace("_", " ").title()
        score = input(f"  {label} (1-5): ").strip()
        if score:
            eval_data[criterion] = int(score)

    notes = input("  Notes (what did this miss?): ").strip()
    if notes:
        eval_data["notes"] = notes

    result["eval_human"] = eval_data

    with open(result_path, "w") as f:
        json.dump(result, f, indent=2)

    print("Saved.")
```

**Step 2: Commit**

```bash
git add research/eval/rubric.py
git commit -m "feat(research): human evaluation helpers — side-by-side display and scoring"
```

---

## Task 10: Wire remaining techniques (stubs)

Create stub files for the remaining 6 techniques so the registry and CLI are complete. Each stub follows the same pattern as breadth-first but with the technique-specific logic. Implement one at a time as we iterate — for now, stubs that raise `NotImplementedError` are fine.

**Files:**
- Create: `research/techniques/iterative_deepening.py`
- Create: `research/techniques/multi_perspective.py`
- Create: `research/techniques/claim_then_verify.py`
- Create: `research/techniques/map_reduce.py`
- Create: `research/techniques/template_guided.py`
- Create: `research/techniques/autonomous_agent.py`
- Modify: `research/harness.py` (register all techniques)

**Step 1: Write each stub**

Each stub file follows:

```python
"""[Technique name]: [one-line description]."""

from typing import Any
from research.config import Config
from research.techniques.base import Technique, ResearchGraph, RunMetadata


class [ClassName](Technique):
    name = "[technique_name]"

    def run(
        self,
        question: str,
        config: Config,
    ) -> tuple[ResearchGraph, RunMetadata, list[dict[str, Any]]]:
        raise NotImplementedError(f"{self.name} not yet implemented")
```

Technique names and classes:

| File | Class | name |
|------|-------|------|
| `iterative_deepening.py` | `IterativeDeepening` | `iterative_deepening` |
| `multi_perspective.py` | `MultiPerspective` | `multi_perspective` |
| `claim_then_verify.py` | `ClaimThenVerify` | `claim_then_verify` |
| `map_reduce.py` | `MapReduce` | `map_reduce` |
| `template_guided.py` | `TemplateGuided` | `template_guided` |
| `autonomous_agent.py` | `AutonomousAgent` | `autonomous_agent` |

**Step 2: Register in `harness.py`**

Add imports and registry entries:

```python
from research.techniques.iterative_deepening import IterativeDeepening
from research.techniques.multi_perspective import MultiPerspective
from research.techniques.claim_then_verify import ClaimThenVerify
from research.techniques.map_reduce import MapReduce
from research.techniques.template_guided import TemplateGuided
from research.techniques.autonomous_agent import AutonomousAgent

TECHNIQUES: dict[str, type[Technique]] = {
    "breadth_first": BreadthFirst,
    "iterative_deepening": IterativeDeepening,
    "multi_perspective": MultiPerspective,
    "claim_then_verify": ClaimThenVerify,
    "map_reduce": MapReduce,
    "template_guided": TemplateGuided,
    "autonomous_agent": AutonomousAgent,
}
```

**Step 3: Verify CLI lists all techniques**

Run: `cd research && python -m research --list`
Expected: all 7 techniques listed

**Step 4: Commit**

```bash
git add research/techniques/ research/harness.py
git commit -m "feat(research): stub all 7 techniques in registry"
```

---

## Task 11: End-to-end smoke test

This task validates the full pipeline with live infrastructure.

**Step 1: Verify SearXNG is reachable**

Run: `curl -s "${SEARXNG_URL:-http://localhost:8888}/search?q=test&format=json" | python -m json.tool | head -5`
Expected: JSON response with `results` array

**Step 2: Verify ArchiveBox is reachable**

Run: `ls "${ARCHIVEBOX_DATA_DIR:-/opt/archivebox/data}/index.sqlite3"`
Expected: file exists

**Step 3: Run breadth-first on a single stable question**

Run: `cd research && python -m research -t breadth_first -q q2_rcv_arguments`

Expected: completes in 30-120s, prints summary, saves JSON to `results/`.

**Step 4: Run judge on the result**

Run: `cd research && python -c "from research.eval.judge import judge_results_dir; judge_results_dir('results/')"`

Expected: prints scores for each criterion.

**Step 5: Inspect the output**

Run: `cd research && python -c "from research.eval.rubric import compare_results; compare_results('results/', 'q2_rcv_arguments')"`

Expected: prints formatted summary with claims, synthesis, scores.

**Step 6: Commit test results (optional, for baseline)**

```bash
git add research/results/
git commit -m "data(research): baseline breadth-first run on RCV question"
```
