"""Synthetic dataset generation."""

from __future__ import annotations

import random
from pathlib import Path
from collections.abc import Sequence
from typing import Iterator, TypeVar

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


def generate_docs(
    output_path: Path,
    count: int,
    seed: int = 13,
) -> None:
    """Generate synthetic civic discourse documents."""
    if count < 0:
        raise ValueError("count must be non-negative")

    docs = (doc.model_dump() for doc in _doc_records(count=count, seed=seed))
    write_jsonl(output_path, docs)
