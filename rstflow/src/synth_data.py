"""Synthetic dataset generation."""

from __future__ import annotations

import json
import random
import re
from collections.abc import Iterator, Sequence
from pathlib import Path
from typing import TypeVar

import requests

try:  # pragma: no cover - import fallbacks for direct module usage
    from .io_utils import write_jsonl
    from .schemas.documents import RawDocument
except ImportError:  # pragma: no cover
    from io_utils import write_jsonl  # type: ignore
    from schemas.documents import RawDocument  # type: ignore

CONNECTORS = [
    "However",
    "Because",
    "Instead",
    "Since",
    "But",
    "Meanwhile",
    "Therefore",
    "Still",
    "Moreover",
]

STANCES = {
    "pro": [
        "{connector} the ban is enforced, deliveries finally have predictable curb space.",
        "{connector} the traffic study shows a drop in near-miss collisions, the restriction looks sensible.",
        "{connector} restricting parking overnight makes it easier for emergency vehicles to reach {location}.",
    ],
    "anti": [
        "{connector} residents rely on that block for overnight parking, the proposal guts our housing stability.",
        "{connector} the shop owners already lost foot traffic after the bike lanes, this piles on.",
        "{connector} working the late shift leaves us nowhere to keep a car without paying steep garage rates.",
    ],
    "mixed": [
        "{connector} I support cleaner streets, I worry the ban punishes renters without garages.",
        "{connector} the data backs up the safety argument, the rollout misses any mention of low-income permits.",
        "{connector} we need calmer traffic, the city should pair the ban with better night buses.",
    ],
}

STYLE_OPENERS = {
    "resident": [
        "As a longtime resident near {location}, I hear this debate every weekend.",
        "Living a block off {location} means I see the policy impacts up close.",
    ],
    "business_owner": [
        "Running a small shop on {location} keeps me tuned into the curb drama.",
        "Owning a cafe on {location} makes curb access existential for deliveries.",
    ],
    "commuter": [
        "My nightly commute ends on {location}, so this ban hits my routine directly.",
        "After parking along {location} for ten years, I have thoughts on this ban.",
    ],
}

TOPICS = [
    {
        "topic_id": "topic.parking-ban-main-st",
        "location": "Main Street",
        "issue_detail": [
            "block-long snowbanks that push cars into the travel lane",
            "ambulances weaving around ride-share cars at 1 a.m.",
            "loaders scraping mirrors because the curb stays jammed after midnight",
        ],
        "benefits": [
            "city sweepers finally clearing the gutter before the morning rush",
            "overnight delivery trucks staging without double-parking",
            "neighbors walking home without squeezing between bumpers",
        ],
        "drawbacks": [
            "there is no residential garage capacity within six blocks",
            "we do not have reliable night buses for service workers",
            "the permit process is confusing and capped for renters",
        ],
        "actions": [
            "phase the rollout by block so people can adjust",
            "pair the ban with discounted off-street options",
            "add loading zones that flip to resident permits overnight",
        ],
    },
    {
        "topic_id": "topic.bike-boulevard-elm",
        "location": "Elm Boulevard",
        "issue_detail": [
            "delivery vans blocking the greenway before dawn",
            "parents with cargo bikes balancing kids at dusk",
            "teenagers weaving scooters between buses and parked cars",
        ],
        "benefits": [
            "school drop-offs becoming calmer around the crosswalks",
            "late-night restaurants opening patios without exhaust fumes",
            "elderly neighbors finally trusting the intersection signals",
        ],
        "drawbacks": [
            "the city still has potholes that swallow bike tires",
            "the freight corridor needs timed access windows",
            "ride-share staging spills onto the residential side streets",
        ],
        "actions": [
            "share weekly curb monitoring data with the neighborhood council",
            "fund ambassadors who remind drivers about the diversion plan",
            "launch a shuttle so shift workers can leave cars at the garage",
        ],
    },
]

CLOSINGS = [
    "{connector} the council takes this up next week, I hope they {action}.",
    "{connector} we vote on the pilot in two weeks, let's at least {action}.",
    "{connector} we push this to a pilot, we need to {action} first.",
]


T = TypeVar("T")


def _pick(rng: random.Random, items: Sequence[T]) -> T:
    return rng.choice(items)


def _generate_text(
    rng: random.Random,
    topic: dict[str, list[str] | str],
    stance_key: str,
) -> str:
    location = str(topic["location"])
    style_key = _pick(rng, list(STYLE_OPENERS.keys()))

    opener_template = _pick(rng, STYLE_OPENERS[style_key])
    opener = opener_template.format(location=location)

    connectors = rng.sample(CONNECTORS, k=3)
    stance_template = _pick(rng, STANCES[stance_key])
    stance_sentence = stance_template.format(
        connector=connectors[0],
        location=location,
    )

    nuance_sentence = (
        f"{connectors[1]} { _pick(rng, topic['benefits']) }, yet { _pick(rng, topic['drawbacks']) }."
    )

    closing = _pick(rng, CLOSINGS).format(
        connector=connectors[2],
        action=_pick(rng, topic["actions"]),
    )

    detail_clause = (
        f"{_pick(rng, ['Lately', 'This winter', 'Over the past month'])}, "
        f"I keep running into {_pick(rng, topic['issue_detail'])}."
    )

    return " ".join([opener, detail_clause, stance_sentence, nuance_sentence, closing])


def _doc_records(
    count: int,
    seed: int,
) -> Iterator[RawDocument]:
    rng = random.Random(seed)

    for idx in range(1, count + 1):
        topic = _pick(rng, TOPICS)
        stance_key = _pick(rng, list(STANCES.keys()))
        text = _generate_text(rng, topic, stance_key)
        doc = RawDocument(
            doc_id=f"d{idx:06d}",
            topic_id=str(topic["topic_id"]),
            author_id=f"u_{rng.randint(0, 9999):04d}",
            text=text,
        )
        yield doc


def _generate_template_docs(count: int, seed: int) -> Iterator[dict[str, str]]:
    for doc in _doc_records(count=count, seed=seed):
        yield doc.model_dump()


def _call_lmstudio(
    *,
    count: int,
    base_url: str,
    model: str,
    temperature: float,
    timeout: int,
) -> list[dict[str, str]]:
    system_prompt = "You generate civic discourse comments in JSON Lines format."
    user_prompt = (
        f"Create exactly {count} JSON Lines entries about dense urban parking debates. "
        "Each line must be a JSON object with keys doc_id (d000001 onwards), "
        "topic_id (topic.parking-ban-main-st or topic.bike-boulevard-elm), "
        "author_id (u_0001 style), and text (2-3 sentences using connectors like "
        "However, Because, Instead). Reply with ONLY those JSON objects, no markdown or prose."
    )

    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
        "temperature": temperature,
        "max_tokens": 4096,
    }

    response = requests.post(base_url, json=payload, timeout=timeout)
    response.raise_for_status()
    content = response.json()["choices"][0]["message"]["content"].strip()
    normalized = re.sub(r"}\s*{", "}\n{", content)
    lines = [line.strip() for line in normalized.splitlines() if line.strip()]

    if len(lines) != count:
        raise RuntimeError(f"Expected {count} lines from LM Studio, got {len(lines)}")

    records: list[dict[str, str]] = []
    for line in lines:
        parsed = json.loads(line)
        doc = RawDocument.model_validate(parsed)
        records.append(doc.model_dump())

    return records


def generate_docs(
    output_path: Path,
    count: int,
    seed: int = 13,
    backend: str = "template",
    *,
    lmstudio_url: str = "http://127.0.0.1:1234/v1/chat/completions",
    lmstudio_model: str = "openai/gpt-oss-20b",
    temperature: float = 0.8,
    timeout: int = 120,
) -> None:
    """Generate synthetic civic discourse documents."""
    if count < 0:
        raise ValueError("count must be non-negative")

    if backend == "template":
        docs = _generate_template_docs(count=count, seed=seed)
    elif backend == "lmstudio":
        docs = _call_lmstudio(
            count=count,
            base_url=lmstudio_url,
            model=lmstudio_model,
            temperature=temperature,
            timeout=timeout,
        )
    else:
        raise ValueError(f"Unsupported backend '{backend}'")

    write_jsonl(output_path, docs)
