#!/usr/bin/env python3
"""
TinyCongress: single-script demo pipeline

Flow:
  1) Create personas via LLM
  2) Create topic + phase + prompt via LLM
  3) For each persona, generate a response
  4) Extract canonical propositions from the collected responses
  5) For each persona, label A/D/P over each proposition via LLM
  6) Build A/D/P matrix; cluster participants
  7) Compute and print bridging scores for propositions

All LLM calls go to a single, OpenAI-compatible endpoint:
  POST http://localhost:1234/v1/chat/completions
"""

import json
import time
import random
import textwrap
import re
import sys
import uuid
from collections import Counter
from typing import List, Dict, Any, Tuple, Optional
import click
import requests
import numpy as np
from loguru import logger
from sklearn.cluster import KMeans, AgglomerativeClustering
from sklearn.decomposition import PCA
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.metrics import silhouette_score
from sklearn.metrics.pairwise import cosine_distances, cosine_similarity

# --------------------------- Configuration ---------------------------

LLM_ENDPOINT = "http://localhost:1234/v1/chat/completions"
LLM_MODEL = "qwen/qwen3-1.7b"
TEMPERATURE = 0.4
MAX_TOKENS = -1
STREAM = False

RANDOM_SEED = 42
NUM_PERSONAS = 6              # Keep modest for a quick demo
NUM_PROPOSITIONS = 6          # Target count; LLM may return fewer if deduped
K_CLUSTERS = 2                # Cluster personas into 2 groups for bridging calc
REQUEST_TIMEOUT = 60

DEFAULT_LOG_LEVEL = "TRACE"
LOG_LEVEL_CHOICES = [
    "TRACE",
    "DEBUG",
    "INFO",
    "SUCCESS",
    "WARNING",
    "ERROR",
    "CRITICAL",
]

AX_ECON = ["left", "center", "right"]
AX_SOCIAL = ["liberal", "moderate", "conservative"]
AX_TECH = ["skeptic", "pragmatic", "booster"]
AX_GOVERNANCE = ["localist", "federalist", "market-first"]
COMM_STYLES = [
    "blunt",
    "diplomatic",
    "data-driven",
    "narrative",
    "pugilistic",
    "legalistic",
    "populist",
]
_DISALLOWED_NAME_TOKENS = {
    "justice",
    "reform",
    "freedom",
    "safety",
    "planning",
    "federalist",
    "bias",
    "development",
    "advancement",
}


def append_no_think(content: str) -> str:
    """Append /no_think control token while trimming trailing whitespace."""
    return content.strip() + "\n/no_think"


def _configure_logger(level: str = DEFAULT_LOG_LEVEL) -> None:
    logger.remove()
    logger.add(
        sys.stderr,
        level=level.upper(),
        enqueue=False,
        colorize=True,
        format="<green>{time:YYYY-MM-DD HH:mm:ss.SSS}</green> | <level>{level:<8}</level> | {message}",
    )


_configure_logger()


@click.command()
@click.option(
    "--log-level",
    type=click.Choice(LOG_LEVEL_CHOICES, case_sensitive=False),
    default=DEFAULT_LOG_LEVEL,
    help="Set log verbosity (default: TRACE).",
    show_default=False,
)
def cli(log_level: str) -> None:
    _configure_logger(log_level)
    logger.info("cli:start | log_level={}", log_level.upper())
    main()


def _truncate(value: Any, limit: int = 280) -> str:
    text = str(value)
    return text if len(text) <= limit else text[: limit - 3] + "..."


def tfidf_embed(strs: List[str]) -> np.ndarray:
    if not strs:
        return np.zeros((0, 0))
    vectorizer = TfidfVectorizer(min_df=1, ngram_range=(1, 2))
    return vectorizer.fit_transform(strs).toarray()


def mmr_select(items: List[str], emb_fn, k: int, lambda_div: float = 0.65) -> List[str]:
    if not items:
        return []
    if k >= len(items):
        return items
    embeddings = emb_fn(items)
    if embeddings.size == 0:
        return items[:k]
    selected: List[int] = []
    candidates = list(range(len(items)))
    seed_idx = max(candidates, key=lambda idx: len(items[idx]))
    selected.append(seed_idx)
    candidates.remove(seed_idx)
    while len(selected) < min(k, len(items)) and candidates:
        sims_to_selected = cosine_similarity(embeddings[candidates], embeddings[selected]).max(axis=1)
        sims_global = cosine_similarity(embeddings[candidates], embeddings).mean(axis=1)
        mmr_scores = lambda_div * (1.0 - sims_to_selected) + (1.0 - lambda_div) * (1.0 - sims_global)
        pick = candidates[int(np.argmax(mmr_scores))]
        selected.append(pick)
        candidates.remove(pick)
    return [items[i] for i in selected]


def _tfidf_sim(a: str, b: str) -> float:
    if not a or not b:
        return 0.0
    vec = tfidf_embed([a, b])
    if vec.size == 0:
        return 0.0
    return float(cosine_similarity(vec)[0, 1])


def _is_person_name(name: str) -> bool:
    if not name:
        return False
    tokens = name.strip().split()
    if len(tokens) < 2:
        return False
    if any(tok.lower() in _DISALLOWED_NAME_TOKENS for tok in tokens):
        return False
    return all(tok[0].isupper() for tok in tokens if tok)


def _persona_summary(persona: Dict[str, Any]) -> str:
    axes = persona.get("axes", {})
    priors = persona.get("priors", [])
    return (
        f"{persona.get('name', '').strip()} — {axes.get('economic','?')}/{axes.get('social','?')} "
        f"| {'; '.join(priors[:2])}"
    )


def _validate_persona_data(
    data: Dict[str, Any],
    econ: str,
    social: str,
    tech: str,
    governance: str,
) -> bool:
    try:
        name = data.get("name", "").strip()
        if not _is_person_name(name):
            return False
        axes = data.get("axes", {}) or {}
        if not isinstance(axes, dict):
            return False
        if (
            axes.get("economic") != econ
            or axes.get("social") != social
            or axes.get("tech") != tech
            or axes.get("governance") != governance
        ):
            return False
        if data.get("communication_style") not in COMM_STYLES:
            return False
        values = data.get("values")
        if not isinstance(values, list) or len(values) < 4:
            return False
        priors = data.get("priors")
        if not isinstance(priors, list) or len(priors) < 3:
            return False
        background = data.get("background", "").strip()
        if not background:
            return False
        expertise = data.get("expertise")
        if not isinstance(expertise, list) or not expertise:
            return False
        return True
    except Exception:
        return False

# --------------------------- LLM Client ------------------------------

def call_llm(
    messages: List[Dict[str, str]],
    temperature: float = TEMPERATURE,
    top_p: Optional[float] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
) -> str:
    """
    Calls the OpenAI-compatible Chat Completions API and returns assistant content.
    """
    logger.trace(
        "call_llm:start | model={} | temperature={} | top_p={} | presence={} | frequency={} | message_count={}",
        LLM_MODEL,
        temperature,
        top_p,
        presence_penalty,
        frequency_penalty,
        len(messages),
    )
    logger.trace("call_llm:messages | {}", _truncate(messages))
    payload = {
        "model": LLM_MODEL,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": MAX_TOKENS,
        "stream": STREAM,
    }
    if top_p is not None:
        payload["top_p"] = top_p
    if presence_penalty is not None:
        payload["presence_penalty"] = presence_penalty
    if frequency_penalty is not None:
        payload["frequency_penalty"] = frequency_penalty
    logger.trace("call_llm:payload | {}", _truncate(payload))
    start_time = time.time()
    try:
        resp = requests.post(
            LLM_ENDPOINT,
            headers={"Content-Type": "application/json"},
            data=json.dumps(payload),
            timeout=REQUEST_TIMEOUT,
        )
    except requests.RequestException as exc:
        logger.error("call_llm:request_error | {}", exc)
        raise
    elapsed = time.time() - start_time
    logger.trace(
        "call_llm:response_meta | status={} | elapsed={:.3f}s",
        resp.status_code,
        elapsed,
    )
    resp.raise_for_status()
    data = resp.json()
    content = data["choices"][0]["message"]["content"]
    logger.trace("call_llm:content | {}", _truncate(content))
    return content

def _parse_json_from_llm(content: str) -> Any:
    """Attempt to recover JSON from a possibly noisy LLM response."""
    logger.trace("parse_json:start | content_preview={}", _truncate(content))
    if content is None:
        logger.error("parse_json:error | response is None")
        raise ValueError("LLM response was None")

    text = content.strip()
    if not text:
        logger.error("parse_json:error | response empty after strip")
        raise ValueError("LLM response was empty")

    try:
        parsed = json.loads(text)
        logger.trace("parse_json:success | strategy=direct")
        return parsed
    except json.JSONDecodeError:
        pass

    # Look for fenced code blocks containing JSON
    fence = re.search(r"```(?:json)?\s*(.*?)\s*```", content, flags=re.DOTALL | re.IGNORECASE)
    if fence:
        logger.trace("parse_json:trying | strategy=fenced_block")
        candidate = fence.group(1).strip()
        if candidate:
            parsed = json.loads(candidate)
            logger.trace("parse_json:success | strategy=fenced_block")
            return parsed

    # Walk the string looking for a JSON object/array with raw_decode
    decoder = json.JSONDecoder()
    for match in re.finditer(r"[\[{]", content):
        try:
            obj, _ = decoder.raw_decode(content[match.start():])
            logger.trace("parse_json:success | strategy=raw_decode | index={}", match.start())
            return obj
        except json.JSONDecodeError:
            continue

    logger.error("parse_json:error | unable to recover JSON")
    raise ValueError("Could not extract JSON from LLM response")


def llm_json(messages: List[Dict[str, str]], temperature: float = TEMPERATURE) -> Any:
    """
    Calls the LLM and parses the content as JSON. Retries once with a
    'respond in strict JSON' reminder if parsing fails. Provides clearer
    error context when decoding cannot be recovered.
    """
    logger.trace("llm_json:start | message_count={}", len(messages))
    content = call_llm(messages, temperature)
    try:
        return _parse_json_from_llm(content)
    except ValueError:
        logger.warning("llm_json:retry | reason=parse_error")
        messages2 = messages + [
            {"role": "user", "content": "Respond again in STRICT JSON only, no prose."}
        ]
        content2 = call_llm(messages2, temperature)
        try:
            return _parse_json_from_llm(content2)
        except ValueError as err:
            snippet1 = content.strip().replace("\n", " ")[:200]
            snippet2 = content2.strip().replace("\n", " ")[:200]
            logger.error(
                "llm_json:failure | first_preview={} | second_preview={}",
                snippet1,
                snippet2,
            )
            raise RuntimeError(
                "Failed to decode JSON from LLM response. "
                f"First attempt: '{snippet1}'. Second attempt: '{snippet2}'."
            ) from err

# --------------------------- Prompt Helpers --------------------------

SYSTEM_BASE = {
    "role": "system",
    "content": (
        "You are a neutral deliberation facilitator. "
        "Follow instructions precisely. When asked for JSON, return STRICT JSON."
    ),
}

def generate_persona_for_axes(
    econ: str,
    social: str,
    tech: str,
    governance: str,
    prior_summaries: List[str],
) -> Optional[Dict[str, Any]]:
    schema_text = textwrap.dedent(
        """Schema (STRICT):
        {
          "name": "Firstname Lastname",
          "background": "1 sentence; job and place.",
          "values": ["...", "...", "...", "..."],
          "expertise": ["policy","tech","community","healthcare","law","economics","education","environment","security","faith","disability","rural_dev","transit_ops"],
          "communication_style": "one of {comm}",
          "priors": ["bullet 1","bullet 2","bullet 3"],
          "axes": {
            "economic": "one of {econ}",
            "social": "one of {soc}",
            "tech": "one of {tech}",
            "governance": "one of {gov}"
          }
        }
        """
    ).format(
        comm=COMM_STYLES,
        econ=AX_ECON,
        soc=AX_SOCIAL,
        tech=AX_TECH,
        gov=AX_GOVERNANCE,
    )

    for attempt in range(1, 4):
        nonce = str(uuid.uuid4())
        existing = "\n".join(f"- {s}" for s in prior_summaries[-6:]) or "(none yet)"
        sys = {
            "role": "system",
            "content": "You fabricate one realistic civic persona. Return STRICT JSON only; no prose.",
        }
        user = {
            "role": "user",
            "content": append_no_think(
                textwrap.dedent(
                    f"""
                    {schema_text}

                    Hard constraints:
                    - Name must be a human person's name, not a concept.
                    - Axes must match exactly: economic={econ}, social={social}, tech={tech}, governance={governance}.
                    - Do not copy any existing persona's name or role.
                    - Priors must imply disagreement with at least one other quadrant and be concrete.

                    Existing personas (names and one-line roles); avoid duplication:
                    {existing}

                    Generate ONE persona meeting the schema and constraints above.
                    Return STRICT JSON only. Nonce: {nonce}
                    """
                )
            ),
        }

        temperature = 0.7 + 0.2 * random.random()
        top_p = 0.8 + 0.15 * random.random()
        presence_penalty = round(random.uniform(0.0, 0.7), 2)
        frequency_penalty = round(random.uniform(0.0, 0.5), 2)

        try:
            response = call_llm(
                [sys, user],
                temperature=temperature,
                top_p=top_p,
                presence_penalty=presence_penalty,
                frequency_penalty=frequency_penalty,
            )
            data = _parse_json_from_llm(response)
        except Exception as exc:
            logger.warning(
                "persona:llm_error | econ={} | social={} | attempt={} | error={}",
                econ,
                social,
                attempt,
                exc,
            )
            continue

        if not isinstance(data, dict):
            continue

        if not _validate_persona_data(data, econ, social, tech, governance):
            logger.debug(
                "persona:validation_fail | name={} | axes={}",
                data.get("name"),
                data.get("axes"),
            )
            continue

        pack = (
            (data.get("name", "") or "")
            + " "
            + (data.get("background", "") or "")
            + " "
            + " ".join(data.get("priors", []))
        )
        too_close = any(_tfidf_sim(pack, summary) > 0.7 for summary in prior_summaries)
        if too_close:
            logger.debug("persona:similarity_reject | name={}".format(data.get("name")))
            continue

        return data

    logger.warning(
        "persona:exhausted_attempts | econ={} | social={} | tech={} | governance={}",
        econ,
        social,
        tech,
        governance,
    )
    return None


def gen_personas(n: int = NUM_PERSONAS) -> List[Dict[str, Any]]:
    """Generate diverse personas by sampling per-axis with rejection checks."""
    logger.info("personas:generate | target={}".format(n))

    combos = [
        ("left", "liberal"),
        ("center", "liberal"),
        ("right", "liberal"),
        ("left", "moderate"),
        ("center", "moderate"),
        ("right", "moderate"),
        ("left", "conservative"),
        ("center", "conservative"),
        ("right", "conservative"),
    ]

    plan: List[Tuple[str, str, str, str]] = []
    for i in range(max(n, len(combos))):
        econ, social = combos[i % len(combos)]
        tech = AX_TECH[i % len(AX_TECH)]
        governance = AX_GOVERNANCE[i % len(AX_GOVERNANCE)]
        plan.append((econ, social, tech, governance))
        if len(plan) >= n:
            break

    personas: List[Dict[str, Any]] = []
    summaries: List[str] = []

    for econ, social, tech, governance in plan:
        persona = generate_persona_for_axes(econ, social, tech, governance, summaries)
        if not persona:
            continue

        axes = persona.get("axes", {})
        if not axes or not all(axes.get(k) for k in ["economic", "social", "tech", "governance"]):
            logger.debug("persona:missing_axes | name={}".format(persona.get("name")))
            continue
        if persona.get("communication_style") not in COMM_STYLES:
            logger.debug("persona:bad_style | name={}".format(persona.get("name")))
            continue

        pack = (
            persona.get("name", "")
            + " "
            + persona.get("background", "")
            + " "
            + " ".join(persona.get("priors", []))
        )
        if any(_tfidf_sim(pack, summary) > 0.7 for summary in summaries):
            logger.debug("persona:post_similarity_reject | name={}".format(persona.get("name")))
            continue

        personas.append(persona)
        summaries.append(pack)
        logger.info(
            "persona:accepted | name={} | axes={}/{}/{}/{}",
            persona.get("name"),
            axes.get("economic"),
            axes.get("social"),
            axes.get("tech"),
            axes.get("governance"),
        )

        if len(personas) >= n:
            break

    if len(personas) < n:
        logger.warning("personas:shortfall | have={} | target={}".format(len(personas), n))

    return personas

def gen_topic_phase_prompt() -> Dict[str, str]:
    """
    Ask the LLM for a topic, an initial phase, and a single discussion prompt.
    """
    logger.info("topic:generate")
    user = {
        "role": "user",
        "content": append_no_think(textwrap.dedent("""
        Return STRICT JSON with:
          "topic": choose from {congestion pricing, firearm background checks, short-term rentals, campus protest rules, fentanyl policy, zoning upzoning, police oversight boards}.
          "phase": one of ["brainstorm","triage","focused_deliberation","reflection"]
          "prompt": a neutral question that forces trade-offs (who pays, what freedoms restricted, how enforced, define success metric).
        Also include:
          "disagreement_levers": ["cost allocation","liberty vs safety","equity vs efficiency","local vs federal authority","tech surveillance vs privacy"]
        """))
    }
    tpp = llm_json([SYSTEM_BASE, user])
    topic_val = tpp.get("topic")
    if isinstance(topic_val, list) and topic_val:
        tpp["topic"] = topic_val[0]
    logger.info(
        "topic:received | topic={} | phase={}",
        _truncate(tpp.get("topic")),
        tpp.get("phase"),
    )
    return tpp

def persona_response(
    persona: Dict[str, Any],
    topic: str,
    phase: str,
    prompt: str,
    temperature: float,
    top_p: float,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
) -> str:
    """
    Get a short response from a given persona.
    """
    logger.trace(
        "persona_response:start | name={} | phase={}",
        persona.get("name"),
        phase,
    )
    sys = {
        "role": "system",
        "content": (
            "Reply IN CHARACTER as the persona. Output EXACTLY ONE sentence, 18–26 words, "
            "no preamble, no metadata, no XML, no <think> tags."
        ),
    }
    user = {
        "role": "user",
        "content": append_no_think(textwrap.dedent(f"""
        Persona JSON:
        {json.dumps(persona, ensure_ascii=False)}

        Topic: {topic}
        Phase: {phase}
        Prompt: {prompt}

        Constraints:
        - One sentence only.
        - Expose a nontrivial trade-off tied to this persona's axes.
        - If the majority stance is obvious, push against it consistent with persona priors.
        """))
    }
    response = call_llm(
        [sys, user],
        temperature=temperature,
        top_p=top_p,
        presence_penalty=presence_penalty,
        frequency_penalty=frequency_penalty,
    ).strip()
    response = re.sub(r"</?think>", "", response, flags=re.IGNORECASE).strip()
    logger.trace(
        "persona_response:received | name={} | preview={}",
        persona.get("name"),
        _truncate(response),
    )
    return response

def extract_canonical_propositions(responses: Dict[str, str], k: int = NUM_PROPOSITIONS) -> List[str]:
    """
    From all persona responses, extract ~k canonical, declarative propositions that are
    short, testable, and policy-relevant. Over-generate and prune via Maximal Marginal Relevance.
    """
    logger.info("propositions:extract | target={} | personas={}".format(
        k, len(responses)
    ))
    joined = "\n\n".join([f"{name}: {resp}" for name, resp in responses.items()])
    over_k = max(k * 2, k + 1)
    user = {
        "role": "user",
        "content": append_no_think(textwrap.dedent(f"""
        Given these persona responses:

        {joined}

        Extract approximately {over_k} canonical, atomic propositions that summarize the key
        claims being made (short, declarative, policy-relevant). No duplicates.
        Return STRICT JSON with key "propositions" as a list of strings.
        """))
    }
    data = llm_json([SYSTEM_BASE, user])
    raw_props = data.get("propositions", [])
    logger.info("propositions:raw | count={} | preview={}".format(
        len(raw_props),
        _truncate(raw_props),
    ))
    if len(raw_props) > k:
        try:
            selected = mmr_select(raw_props, tfidf_embed, k=k)
        except Exception as exc:
            logger.warning("propositions:mmr_fallback | {}".format(exc))
            selected = raw_props[:k]
    else:
        selected = raw_props
    propositions = []
    seen = set()
    for s in selected:
        s2 = " ".join(s.split())
        if s2 and s2 not in seen:
            seen.add(s2)
            propositions.append(s2)
    logger.info("propositions:raw | count={} | preview={}".format(
        len(propositions),
        _truncate(propositions),
    ))
    return propositions


def expand_contrastive_propositions(props: List[str]) -> List[str]:
    if not props:
        return []
    user = {
        "role": "user",
        "content": append_no_think(textwrap.dedent(f"""
        Given these propositions:
        {json.dumps(props, ensure_ascii=False)}

        For each, produce ONE concise counter-proposition a reasonable opponent might endorse,
        rotating frames across: liberty, safety, equity, efficiency, property-rights, tenant-rights.
        Return STRICT JSON: {{"counter_propositions": [ ... ]}} in same order.
        """))
    }
    data = llm_json([SYSTEM_BASE, user])
    counters = data.get("counter_propositions", [])
    merged: List[str] = []
    for original, counter in zip(props, counters):
        merged.append(original)
        merged.append(counter)
    seen: set[str] = set()
    out: List[str] = []
    for s in merged:
        s2 = " ".join(s.split())
        if s2 and s2 not in seen:
            seen.add(s2)
            out.append(s2)
    logger.info(
        "propositions:contrastive | initial={} | expanded={}",
        len(props),
        len(out),
    )
    return out

def stance_scalar_for_persona(
    persona_name: str,
    persona_text: str,
    propositions: List[str],
) -> Dict[str, Dict[str, float]]:
    """Request scalar stances with confidence per proposition."""
    logger.trace(
        "stance_scalar:start | persona={} | propositions={}",
        persona_name,
        len(propositions),
    )
    user = {
        "role": "user",
        "content": append_no_think(textwrap.dedent(f"""
        Persona response:
        {persona_name}: {persona_text}

        For EACH proposition below, output:
          - "stance": an integer in [-2,-1,0,1,2] where:
              -2 = strongly disagree, -1 = somewhat disagree, 0 = unsure/neutral,
              +1 = somewhat agree, +2 = strongly agree
          - "confidence": float in [0,1]
        If the persona text does not support a clear stance, use stance=0 with low confidence.

        Propositions:
        {json.dumps(propositions, ensure_ascii=False)}

        Return STRICT JSON:
        {{
          "labels": {{
            "<prop1>": {{"stance": INT, "confidence": FLOAT}},
            ...
          }}
        }}
        """))
    }
    data = llm_json([SYSTEM_BASE, user])
    labels = data.get("labels")
    if not isinstance(labels, dict):
        logger.warning(
            "stance_scalar:missing_labels | persona={} | keys={}",
            persona_name,
            list(data.keys()),
        )
        user_retry = {
            "role": "user",
            "content": append_no_think(textwrap.dedent(f"""
            Persona response:
            {persona_name}: {persona_text}

            You failed to return the required STRICT JSON.
            For EACH proposition below, include an object with keys "stance" (integer in [-2,-1,0,1,2]) and "confidence" (float 0-1).
            If uncertain, set stance=0 and confidence<=0.2.

            Propositions:
            {json.dumps(propositions, ensure_ascii=False)}

            Return STRICT JSON with key "labels" covering every proposition.
            """))
        }
        data = llm_json([SYSTEM_BASE, user_retry])
        labels = data.get("labels")

    if not isinstance(labels, dict):
        logger.error("stance_scalar:fallback_neutral | persona={}".format(persona_name))
        labels = {
            prop: {"stance": 0, "confidence": 0.2}
            for prop in propositions
        }

    logger.trace(
        "stance_scalar:received | persona={} | labels={}",
        persona_name,
        labels,
    )
    return labels

# --------------------------- Math & Analytics ------------------------

def build_scalar_matrix(
    personas: List[str],
    propositions: List[str],
    labels_by_persona: Dict[str, Dict[str, Dict[str, float]]],
) -> np.ndarray:
    logger.debug(
        "matrix:build | personas={} | propositions={}",
        len(personas),
        len(propositions),
    )
    M = np.zeros((len(personas), len(propositions)), dtype=float)
    for i, p_name in enumerate(personas):
        lab = labels_by_persona[p_name]
        for j, prop in enumerate(propositions):
            entry = lab.get(prop, {"stance": 0})
            M[i, j] = float(entry.get("stance", 0))
        logger.trace(
            "matrix:row | persona={} | data={}",
            p_name,
            M[i, :].tolist(),
        )
    return M


def apply_confidence_damping(
    M: np.ndarray,
    labels_by_persona: Dict[str, Dict[str, Dict[str, float]]],
    personas: List[str],
    propositions: List[str],
    floor: float = 0.3,
    extreme_floor: float = 0.9,
) -> np.ndarray:
    logger.debug("matrix:confidence_damping | floor={}".format(floor))
    W = np.ones_like(M, dtype=float)
    for i, name in enumerate(personas):
        lab = labels_by_persona[name]
        for j, prop in enumerate(propositions):
            item = lab.get(prop, {})
            conf = float(item.get("confidence", 0.0))
            stance = float(item.get("stance", 0.0))
            weight = max(conf, floor)
            if abs(stance) == 2 and conf < extreme_floor:
                weight *= 0.6
            W[i, j] = weight
    damped = M * W
    return damped


def diversify_if_collapsed(
    responses: Dict[str, str],
    personas: List[Dict[str, Any]],
    seed: int = RANDOM_SEED,
) -> Dict[str, str]:
    names = list(responses.keys())
    texts = [responses[n] for n in names]
    embeddings = tfidf_embed(texts)
    if embeddings.size == 0:
        return responses
    sims = cosine_similarity(embeddings)
    np.fill_diagonal(sims, 0.0)
    mean_sim = sims.mean()
    logger.debug("responses:mean_similarity | {:.3f}".format(mean_sim))
    if mean_sim <= 0.75:
        return responses
    rng = np.random.default_rng(seed)
    idxs = rng.choice(len(names), size=max(1, len(names) // 2), replace=False)
    logger.info("responses:diversify_if_collapsed | count={}".format(len(idxs)))
    updated = responses.copy()
    for idx in idxs:
        name = names[idx]
        persona = next((p for p in personas if p.get("name") == name), None)
        axes = persona.get("axes", {}) if persona else {}
        msg = {
            "role": "user",
            "content": append_no_think(textwrap.dedent(f"""
            Rewrite this single sentence AS THE REASONABLE OPPONENT of the persona, consistent with these axes:
            axes={json.dumps(axes, ensure_ascii=False)}
            Original: {json.dumps(responses[name], ensure_ascii=False)}
            Return one sentence, 18–26 words, no metadata.
            """))
        }
        rewritten = call_llm([SYSTEM_BASE, msg], temperature=0.7, top_p=0.8).strip()
        rewritten = re.sub(r"</?think>", "", rewritten, flags=re.IGNORECASE).strip()
        updated[name] = rewritten
        logger.trace("responses:diversify | persona={} | text={}".format(name, rewritten))
    return updated


def sharpen_pairs(
    responses: Dict[str, str],
    personas: List[Dict[str, Any]],
    seed: int = RANDOM_SEED,
) -> Dict[str, str]:
    names = list(responses.keys())
    texts = [responses[n] for n in names]
    embeddings = tfidf_embed(texts)
    if embeddings.size == 0:
        return responses
    sims = cosine_similarity(embeddings)
    updated = responses.copy()
    persona_map = {p.get("name"): p for p in personas}
    for i in range(len(names)):
        for j in range(i + 1, len(names)):
            if sims[i, j] > 0.85:
                axes = persona_map.get(names[j], {}).get("axes", {})
                msg = {
                    "role": "user",
                    "content": append_no_think(textwrap.dedent(f"""
                    Rewrite this sentence to clearly diverge from the other persona's priorities,
                    consistent with your axes {json.dumps(axes, ensure_ascii=False)} and priors; 18–26 words, one sentence, no metadata.
                    Yours: {json.dumps(responses[names[j]], ensure_ascii=False)}
                    Other: {json.dumps(responses[names[i]], ensure_ascii=False)}
                    """))
                }
                rewritten = call_llm([SYSTEM_BASE, msg], temperature=0.8, top_p=0.85).strip()
                rewritten = re.sub(r"</?think>", "", rewritten, flags=re.IGNORECASE).strip()
                updated[names[j]] = rewritten
                logger.trace("responses:sharpen | pair=({}, {}) | text={}".format(names[i], names[j], rewritten))
    return updated


def devil_advocate_rewrite(
    responses: Dict[str, str],
    personas: List[str],
    seed: int = RANDOM_SEED,
) -> Dict[str, str]:
    if not responses:
        return responses
    rng = random.Random(seed)
    sample = rng.sample(personas, k=max(1, len(personas) // 2))
    logger.info("devils_advocate:rewrite | count={}".format(len(sample)))
    updated = responses.copy()
    for name in sample:
        user = {
            "role": "user",
            "content": append_no_think(textwrap.dedent(f"""
            Rewrite this single sentence in the style of the persona's likely opponent; keep it one sentence, 18–30 words, no metadata, no XML:
            {json.dumps(responses[name], ensure_ascii=False)}
            Return ONLY the rewritten sentence.
            """))
        }
        rewritten = call_llm([SYSTEM_BASE, user], temperature=0.7, top_p=0.85).strip()
        rewritten = re.sub(r"</?think>", "", rewritten, flags=re.IGNORECASE).strip()
        updated[name] = rewritten
        logger.trace("devils_advocate:persona | name={} | text={}".format(name, rewritten))
    return updated


def enforce_counterstance_if_uniform(
    M: np.ndarray,
    labels_by_persona: Dict[str, Dict[str, Dict[str, float]]],
    responses: Dict[str, str],
    persona_records: List[Dict[str, Any]],
    propositions: List[str],
    threshold: float = 1.6,
    seed: int = RANDOM_SEED,
) -> Tuple[Dict[str, str], bool]:
    if M.size == 0:
        return responses, False
    rng = random.Random(seed)
    means = M.mean(axis=0)
    updated = responses.copy()
    adjusted = False
    persona_map = {p.get("name"): p for p in persona_records}
    for idx, mean_val in enumerate(means):
        if abs(mean_val) <= threshold:
            continue
        majority_sign = 1 if mean_val >= 0 else -1
        prop = propositions[idx]
        candidates: List[Tuple[float, str]] = []
        for name, labels in labels_by_persona.items():
            entry = labels.get(prop, {})
            stance = float(entry.get("stance", 0))
            conf = float(entry.get("confidence", 0))
            if np.sign(stance) == majority_sign or stance == 0:
                candidates.append((conf, name))
        if not candidates:
            continue
        candidates.sort(key=lambda x: (x[0], rng.random()))
        _, target_name = candidates[0]
        persona = persona_map.get(target_name, {})
        axes = persona.get("axes", {})
        msg = {
            "role": "user",
            "content": append_no_think(textwrap.dedent(f"""
            Persona JSON:
            {json.dumps(persona, ensure_ascii=False)}

            Proposition with overwhelming agreement:
            {json.dumps(prop, ensure_ascii=False)}

            Your previous sentence:
            {json.dumps(responses.get(target_name, ''), ensure_ascii=False)}

            Rewrite a single 18–26 word sentence that credibly argues the opposing stance while staying authentic to your axes and priors.
            Emphasize the trade-off your persona prioritizes.
            """))
        }
        rewritten = call_llm([SYSTEM_BASE, msg], temperature=0.75, top_p=0.75).strip()
        rewritten = re.sub(r"</?think>", "", rewritten, flags=re.IGNORECASE).strip()
        updated[target_name] = rewritten
        adjusted = True
        logger.info(
            "responses:counterstance | persona={} | prop={} | mean={:.2f}",
            target_name,
            prop,
            mean_val,
        )
    return updated, adjusted

def cluster_personas(M: np.ndarray, k: int = K_CLUSTERS, seed: int = RANDOM_SEED) -> Tuple[np.ndarray, float]:
    logger.debug(
        "cluster:start | k={} | matrix_shape={}",
        k,
        M.shape,
    )
    if M.size == 0:
        return np.zeros(0, dtype=int), -1.0

    if np.allclose(M.var(axis=0).sum(), 0.0):
        rng = np.random.default_rng(seed)
        M = M + rng.normal(0, 1e-3, M.shape)

    try:
        km = KMeans(n_clusters=k, random_state=seed, n_init="auto")
        labels = km.fit_predict(M)
        if len(set(labels)) < 2:
            raise RuntimeError("Degenerate KMeans result")
        sil = silhouette_score(M, labels, metric="cosine")
        logger.debug("cluster:result | method=kmeans | silhouette={:.3f}".format(sil))
        return labels, sil
    except Exception as exc:
        logger.warning("cluster:kmeans_fallback | reason={}".format(exc))
        D = cosine_distances(M + 1e-9)
        try:
            agg = AgglomerativeClustering(
                n_clusters=min(k, len(M)),
                affinity="precomputed",
                linkage="average",
            )
            labels = agg.fit_predict(D)
        except Exception as agg_exc:
            logger.warning("cluster:agglomerative_fallback | reason={}".format(agg_exc))
            if M.shape[1] >= 2:
                X = PCA(n_components=2).fit_transform(M)
            else:
                X = np.hstack([M, np.zeros_like(M)])
            km2 = KMeans(n_clusters=min(k, len(M)), random_state=seed, n_init="auto")
            labels = km2.fit_predict(X)

        try:
            sil = silhouette_score(M, labels, metric="cosine") if len(set(labels)) > 1 else -1.0
        except Exception:
            sil = -1.0
        logger.debug("cluster:result | method=fallback | silhouette={:.3f}".format(sil))
        return labels, sil

def bridging_scores(M: np.ndarray, cluster_labels: np.ndarray) -> np.ndarray:
    n_personas, n_props = M.shape
    unique = np.unique(cluster_labels)
    if len(unique) < 2:
        logger.debug("bridging:single_cluster")
        return np.ones(n_props, dtype=float)

    weights = {c: np.mean(cluster_labels == c) for c in unique}
    logger.debug("bridging:start | clusters={} | weights={}".format(unique.tolist(), weights))
    scores = np.zeros(n_props, dtype=float)

    for j in range(n_props):
        means = {c: M[cluster_labels == c, j].mean() for c in unique}
        overall = sum(weights[c] * means[c] for c in unique)
        penalty = sum(weights[c] * abs(means[c] - overall) for c in unique)
        scores[j] = max(0.0, 1.0 - (penalty / 2.0))
        logger.trace(
            "bridging:prop | index={} | means={} | overall={:.3f} | score={:.3f}",
            j,
            means,
            overall,
            scores[j],
        )
    return scores

# --------------------------- Main Pipeline ---------------------------

def main():
    random.seed(RANDOM_SEED)
    np.random.seed(RANDOM_SEED)

    logger.info("STEP | Generating personas")
    personas = gen_personas(NUM_PERSONAS)
    persona_names = [p["name"] for p in personas]
    logger.info(
        "personas:list | count={} | names={}",
        len(personas),
        ", ".join(persona_names),
    )
    econ_axes = Counter(p.get("axes", {}).get("economic", "unknown") for p in personas)
    social_axes = Counter(p.get("axes", {}).get("social", "unknown") for p in personas)
    tech_axes = Counter(p.get("axes", {}).get("tech", "unknown") for p in personas)
    gov_axes = Counter(p.get("axes", {}).get("governance", "unknown") for p in personas)
    logger.info(
        "axes:distribution | economic={} | social={} | tech={} | governance={}",
        dict(econ_axes),
        dict(social_axes),
        dict(tech_axes),
        dict(gov_axes),
    )

    logger.info("STEP | Generating topic, phase, and prompt")
    tpp = gen_topic_phase_prompt()
    topic = tpp["topic"]
    phase = tpp["phase"]
    prompt = tpp["prompt"]
    disagreement_levers = tpp.get("disagreement_levers", [])
    logger.info("topic:payload | {}", json.dumps(tpp, ensure_ascii=False))
    logger.info("topic:disagreement_levers | {}", disagreement_levers)

    logger.info("STEP | Collecting persona responses")
    responses = {}
    for p in personas:
        name = p["name"]
        temp = 0.55 + 0.45 * random.random()
        top_p = 0.65 + 0.30 * random.random()
        presence_penalty = round(random.uniform(0.0, 0.9), 2)
        frequency_penalty = round(random.uniform(0.0, 0.7), 2)
        resp = persona_response(
            p,
            topic,
            phase,
            prompt,
            temperature=temp,
            top_p=top_p,
            presence_penalty=presence_penalty,
            frequency_penalty=frequency_penalty,
        )
        responses[name] = resp
        logger.info(
            "response | persona={} | temp={:.2f} | top_p={:.2f} | presence={:.2f} | freq={:.2f} | text={}",
            name,
            temp,
            top_p,
            presence_penalty,
            frequency_penalty,
            resp,
        )

    responses = diversify_if_collapsed(responses, personas)
    responses = sharpen_pairs(responses, personas)

    logger.info("STEP | Extracting canonical propositions")
    propositions = extract_canonical_propositions(responses, NUM_PROPOSITIONS)
    propositions = expand_contrastive_propositions(propositions)
    # De-dup & trim whitespace just in case
    deduped = []
    seen = set()
    for s in propositions:
        s2 = " ".join(s.split())
        if s2 and s2 not in seen:
            seen.add(s2)
            deduped.append(s2)
    propositions = deduped
    logger.info(
        "propositions:deduped | count={} | data={}",
        len(propositions),
        json.dumps(propositions, ensure_ascii=False),
    )

    logger.info("STEP | Scalar stance labeling per persona")
    labels_by_persona: Dict[str, Dict[str, Dict[str, float]]] = {}
    for name in persona_names:
        lab = stance_scalar_for_persona(name, responses[name], propositions)
        labels_by_persona[name] = lab
        logger.info("labels | persona={} | data={}", name, lab)

    logger.info("STEP | Building scalar matrix")
    M_raw = build_scalar_matrix(persona_names, propositions, labels_by_persona)
    header = ["persona"] + [f"P{j+1}" for j in range(len(propositions))]
    rowfmt = "{:<14}" + " ".join(["{:>4}"] * len(propositions))
    logger.info("matrix:header | {}", " | ".join(header))
    for i, name in enumerate(persona_names):
        row = [name] + [int(M_raw[i, j]) for j in range(len(propositions))]
        logger.info("matrix:row | {}", rowfmt.format(*row))

    responses, counter_adjusted = enforce_counterstance_if_uniform(
        M_raw,
        labels_by_persona,
        responses,
        personas,
        propositions,
    )
    if counter_adjusted:
        logger.info("responses:counterstance_adjusted | re-labeling personas")
        labels_by_persona = {}
        for name in persona_names:
            lab = stance_scalar_for_persona(name, responses[name], propositions)
            labels_by_persona[name] = lab
            logger.info("labels:counterstance | persona={} | data={}", name, lab)
        M_raw = build_scalar_matrix(persona_names, propositions, labels_by_persona)
        for i, name in enumerate(persona_names):
            row = [name] + [int(M_raw[i, j]) for j in range(len(propositions))]
            logger.info("matrix:row_updated | {}", rowfmt.format(*row))

    M = apply_confidence_damping(M_raw, labels_by_persona, persona_names, propositions)
    logger.info("matrix:damped | shape={} | sample_row={}".format(M.shape, M[0].tolist() if M.size else []))

    if M.size and (np.allclose(M, M[0]) or np.allclose(M.var(), 0.0)):
        logger.warning("matrix:degenerate | invoking devil's advocate rewrites")
        responses = devil_advocate_rewrite(responses, persona_names)
        labels_by_persona = {}
        for name in persona_names:
            lab = stance_scalar_for_persona(name, responses[name], propositions)
            labels_by_persona[name] = lab
            logger.info("labels:redo | persona={} | data={}", name, lab)
        M_raw = build_scalar_matrix(persona_names, propositions, labels_by_persona)
        M = apply_confidence_damping(M_raw, labels_by_persona, persona_names, propositions)

    logger.info("STEP | Clustering personas")
    if K_CLUSTERS > len(persona_names):
        k = max(1, len(persona_names) // 2 or 1)
    else:
        k = K_CLUSTERS
    labels, sil = cluster_personas(M, k=k, seed=RANDOM_SEED)
    clusters: Dict[int, List[str]] = {}
    for name, lab in zip(persona_names, labels):
        clusters.setdefault(lab, []).append(name)
    logger.info("cluster:silhouette | {:.3f}", sil)
    for c, names in clusters.items():
        logger.info("cluster:members | cluster={} | names={}", c, ", ".join(names))

    centroids = []
    unique_labels = sorted(set(labels)) if len(labels) else []
    for c in unique_labels:
        centroids.append(M[labels == c].mean(axis=0))
    logger.info("cluster:centroids | count={}", len(centroids))
    for idx, vec in enumerate(centroids):
        logger.info(
            "cluster:centroid | cluster={} | stances={}",
            idx,
            " ".join(f"{v:+.2f}" for v in vec),
        )
    logger.info("topic:disagreement_levers | {}", disagreement_levers)

    logger.info("STEP | Bridging scores for propositions")
    bridge = np.asarray(bridging_scores(M, labels), dtype=float)
    penalty = 1.0 - bridge
    ranked = sorted(list(enumerate(bridge)), key=lambda x: x[1], reverse=True)
    logger.info("bridging:ranked | order={}", [idx + 1 for idx, _ in ranked])
    for idx, score in ranked:
        logger.info(
            "bridging:item | rank={} | score={:.3f} | proposition={}",
            idx + 1,
            score,
            propositions[idx],
        )

    polar_ranked = sorted(list(enumerate(penalty)), key=lambda x: x[1], reverse=True)
    logger.info("polarizing:ranked | order={}", [idx + 1 for idx, _ in polar_ranked])
    for idx, gap in polar_ranked[: min(5, len(propositions))]:
        logger.info(
            "polarizing:item | rank={} | gap={:.3f} | proposition={}",
            idx + 1,
            gap,
            propositions[idx],
        )

    quadrants = {
        (
            p.get("axes", {}).get("economic"),
            p.get("axes", {}).get("social"),
        )
        for p in personas
        if "axes" in p
    }
    logger.info("validation:quadrant_coverage | count={} | combos={}", len(quadrants), list(quadrants))

    if len(responses) > 1:
        response_texts = [responses[name] for name in persona_names]
        embed = tfidf_embed(response_texts)
        if embed.size:
            sims = cosine_similarity(embed)
            tri_upper = sims[np.triu_indices_from(sims, k=1)]
            if tri_upper.size:
                share_low = float((tri_upper < 0.6).sum()) / tri_upper.size
                logger.info("validation:pairwise_low_similarity | {:.1f}%", share_low * 100)

    if bridge.size:
        bridge_mid = [float(s) for s in bridge if 0.4 <= s <= 0.8]
        polar_high = [float(p) for p in penalty if p > 0.5]
        logger.info(
            "validation:bridge_range | midrange={} | polarizing={}",
            len(bridge_mid),
            len(polar_high),
        )

    centroid_separation = 0.0
    if len(centroids) >= 2:
        base = centroids[0]
        for other in centroids[1:]:
            diff = np.abs(base - other)
            centroid_separation = max(centroid_separation, float(diff.max()))
            large = (diff >= 0.8).sum()
            logger.info("validation:centroid_diff | comparisons={} | >=0.8 count={}", diff.size, large)
    logger.info("validation:max_centroid_gap | {:.2f}", centroid_separation)

    logger.info("STEP | Done")

if __name__ == "__main__":
    cli()
