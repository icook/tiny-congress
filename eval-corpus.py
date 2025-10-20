#!/usr/bin/env python3
"""
LM Studio persona evaluation corpus generator.

What it does:
1) Defines 4 biography-first U.S. personas (JSON).
2) Calls LM Studio's OpenAI-compatible API at http://localhost:1234 to list models.
3) Uses fixed models: GEN=deepseek/deepseek-r1-0528-qwen3-8b, JUDGE=qwen/qwen3-next-80b.
4) For each GEN model:
     - For each persona x topic: generate K=3 candidates (≤180 tokens).
     - For each candidate: score with every JUDGE model (pointwise rubric).
5) Writes:
   - results.json (raw runs, timings, scores)
   - report.md (markdown summary for pasting into GPT UI)

Customize:
- LM_STUDIO_URL
- PERSONAS / TOPICS
- K (num candidates per (persona, topic, gen))
- TIMEOUT_S, TEMPS, MAX_TOKENS

Requires: Python 3.9+, 'requests'
"""

import os
import re
import json
import time
import math
import uuid
import statistics as stats
from datetime import datetime
from typing import Any, Dict, List, Optional, Tuple
from urllib.parse import quote

import requests

LM_STUDIO_URL = os.environ.get("LM_STUDIO_URL", "http://localhost:1234")
CHAT_URL = f"{LM_STUDIO_URL}/v1/chat/completions"
MODELS_URL = f"{LM_STUDIO_URL}/v1/models"
GEN_MODEL_IDS = ["deepseek/deepseek-r1-0528-qwen3-8b", "qwen/qwen3-4b-thinking-2507"]
JUDGE_MODEL_IDS = ["qwen/qwen3-next-80b"]

# ------------ Personas (compact, biography-first) -----------------

PERSONAS: List[Dict[str, Any]] = [
    {
        "persona_id": "AL-Agnes-Teacher",
        "demographics": {
            "age": 79, "gender": "female", "race_ethnicity": "white",
            "state": "Alabama", "urbanicity": "suburban", "education": "bachelors", "income_bracket": "40-60k"
        },
        "biography": (
            "Agnes is a 79-year-old retired third-grade teacher from Alabama. She raised three children, "
            "volunteers at her church and food pantry, reads the local paper, and gardens. Pragmatic moderate Democrat: "
            "believes in neighborly help, dislikes government waste."
        ),
        "values_axes": {"economic_justice": 3.0, "authority": -1.0, "tradition": 1.5, "institution_trust": 2.0, "tech_optimism": 0.7},
        "style": {"register": "informal", "tone": "warm, practical", "taboos": ["personal insults", "conspiracy talk"], "lexical_density": 0.6},
        "one_liner": "Retired Alabama teacher; pragmatic church volunteer; neighborly fairness."
    },
    {
        "persona_id": "WA-Carlos-Tech",
        "demographics": {
            "age": 34, "gender": "male", "race_ethnicity": "latino",
            "state": "Washington", "urbanicity": "urban", "education": "masters", "income_bracket": "120-160k"
        },
        "biography": (
            "Carlos is a 34-year-old software engineer in Seattle working on climate data tools. Bikes to work, early adopter, "
            "active in mutual-aid Slack. Politically progressive with a pragmatic streak on budgets."
        ),
        "values_axes": {"economic_justice": 2.0, "authority": -2.0, "tradition": -1.0, "institution_trust": 1.0, "tech_optimism": 3.5},
        "style": {"register": "informal", "tone": "analytic, concise", "taboos": ["ad hominem", "hand-wavy claims"], "lexical_density": 0.7},
        "one_liner": "Progressive Seattle engineer; climate/data pragmatist; pro-bike/pro-tech."
    },
    {
        "persona_id": "KS-Maya-Nurse",
        "demographics": {
            "age": 41, "gender": "female", "race_ethnicity": "black",
            "state": "Kansas", "urbanicity": "suburban", "education": "associates", "income_bracket": "60-80k"
        },
        "biography": (
            "Maya is a 41-year-old RN in the Kansas City metro. Works night shifts, cares for two kids, volunteers at a free clinic. "
            "Moderate on economics, strongly values public health and safety; wary of big promises without plans."
        ),
        "values_axes": {"economic_justice": 1.0, "authority": 0.5, "tradition": 0.5, "institution_trust": 1.5, "tech_optimism": 0.5},
        "style": {"register": "plainspoken", "tone": "empathetic, no-nonsense", "taboos": ["fear-mongering"], "lexical_density": 0.55},
        "one_liner": "KC-area nurse; public health first; practical, empathetic."
    },
    {
        "persona_id": "TX-Dan-Rancher",
        "demographics": {
            "age": 57, "gender": "male", "race_ethnicity": "white",
            "state": "Texas", "urbanicity": "rural", "education": "some_college", "income_bracket": "80-100k"
        },
        "biography": (
            "Dan runs a small cattle operation in Texas. Active in the volunteer fire department. Fiscally conservative, "
            "values local control and personal responsibility; open to public investments if they clearly pencil out."
        ),
        "values_axes": {"economic_justice": -1.5, "authority": 1.5, "tradition": 2.0, "institution_trust": 0.0, "tech_optimism": -0.2},
        "style": {"register": "informal", "tone": "direct, plainspoken", "taboos": ["bureaucratic jargon"], "lexical_density": 0.5},
        "one_liner": "Texas rancher; local-control conservative; show-the-math pragmatist."
    },
]

# Topics (moderate difficulty, civic-ish)
TOPICS = [
    "Convert two downtown lanes into protected bike lanes.",
    "Subsidize heat-pump retrofits for middle-income households.",
    "Allow permit-less concealed carry statewide.",
    "Issue municipal bonds to rebuild water mains and replace lead service lines.",
]

# Generation settings
K = 3  # candidates per persona/topic per GEN model
GEN_TEMPERATURE = 0.8
GEN_TOP_P = 0.9
GEN_MAX_TOKENS = 220
JUDGE_TEMPERATURE = 0.0
JUDGE_TOP_P = 1.0
JUDGE_MAX_TOKENS = 64
TIMEOUT_S = 120

# ----------------- Utility: list & bucket models ------------------

SIZE_PATTERNS = [
    (r"(70|80|90|100)B", "xl"),
    (r"(30|32|34|40)B", "l"),
    (r"(13|14|15|20)B", "m"),
    (r"(6|7|8|9|10|11|12)B", "s"),
    (r"\bQ\d|int\d|GGUF\b", "quant"),  # quant flags (not size)
]

def model_size_bucket(name: str) -> str:
    for pat, tag in SIZE_PATTERNS:
        if re.search(pat, name, flags=re.IGNORECASE):
            return tag
    # heuristic: check "7b", "13b" lowercase
    if re.search(r"\b(80b|70b|100b)\b", name):
        return "xl"
    if re.search(r"\b(30b|32b|34b|40b)\b", name):
        return "l"
    if re.search(r"\b(13b|14b|15b|20b)\b", name):
        return "m"
    if re.search(r"\b(6b|7b|8b|9b|10b|11b|12b)\b", name):
        return "s"
    return "unknown"

def list_models() -> List[str]:
    r = requests.get(MODELS_URL, timeout=TIMEOUT_S)
    r.raise_for_status()
    data = r.json()
    ids = [m["id"] for m in data.get("data", []) if "id" in m]
    return sorted(ids)


def dedupe(items: List[str]) -> List[str]:
    seen = set()
    ordered = []
    for item in items:
        if item not in seen:
            seen.add(item)
            ordered.append(item)
    return ordered


def _model_request(method: str, url: str, payload: Optional[Dict[str, Any]]) -> Tuple[bool, str]:
    try:
        kwargs: Dict[str, Any] = {"timeout": TIMEOUT_S}
        if payload is not None:
            kwargs["json"] = payload
        response = requests.request(method, url, **kwargs)
        if response.status_code in (200, 204):
            return True, ""
        body = response.text.strip().lower()
        # LM Studio returns 409 if already loaded / not loaded; treat those as success.
        if response.status_code == 409:
            if "already" in body or "not loaded" in body:
                return True, ""
        return False, f"{method} {url} -> {response.status_code} {response.text.strip()}"
    except Exception as exc:  # pragma: no cover — defensive network guard
        return False, f"{method} {url} -> {exc}"


def load_model(model_id: str) -> bool:
    model_path = quote(model_id, safe="")
    attempts = [
        ("POST", f"{MODELS_URL}/{model_path}/load", None),
        ("POST", f"{MODELS_URL}/load", {"id": model_id}),
        ("POST", f"{MODELS_URL}/load", {"model": model_id}),
    ]
    errors = []
    for method, url, payload in attempts:
        ok, err = _model_request(method, url, payload)
        if ok:
            return True
        if err:
            errors.append(err)
    if errors:
        print(f"[warn] Could not load model '{model_id}': {'; '.join(errors)}")
    return False


def unload_model(model_id: str) -> None:
    model_path = quote(model_id, safe="")
    attempts = [
        ("POST", f"{MODELS_URL}/{model_path}/unload", None),
        ("POST", f"{MODELS_URL}/unload", {"id": model_id}),
        ("DELETE", f"{MODELS_URL}/{model_path}", None),
    ]
    for method, url, payload in attempts:
        ok, _ = _model_request(method, url, payload)
        if ok:
            return

def bucket_models(model_ids: List[str]) -> Tuple[List[str], List[str]]:
    gens, judges = [], []
    for mid in model_ids:
        bucket = model_size_bucket(mid)
        # default heuristic: <=14B => GEN; >=30B => JUDGE
        if bucket in ("s", "m"):  # s (~7-13B), m (~13-20B)
            gens.append(mid)
        elif bucket in ("l", "xl"):  # l (30-40B), xl (70-100B)
            judges.append(mid)
        else:
            # Fallback: try to parse digits
            if re.search(r"\b(7b|8b|9b|10b|11b|12b|13b|14b)\b", mid, re.I):
                gens.append(mid)
            elif re.search(r"\b(30b|32b|34b|40b|70b|80b|100b)\b", mid, re.I):
                judges.append(mid)
    return gens, judges

# ----------------- Prompt builders ------------------

def build_persona_traits(persona: Dict[str, Any]) -> str:
    # Keep traits compact to reduce judge prefill cost
    obj = {
        "persona_id": persona["persona_id"],
        "demographics": persona["demographics"],
        "values_axes": persona["values_axes"],
        "style": persona["style"],
        "one_liner": persona["one_liner"],
    }
    return json.dumps(obj, ensure_ascii=False)

def build_generator_messages(persona: Dict[str, Any], topic: str) -> List[Dict[str, str]]:
    traits = {
        "persona_id": persona["persona_id"],
        "demographics": persona["demographics"],
        "values_axes": persona["values_axes"],
        "style": persona["style"],
        "one_liner": persona["one_liner"],
    }
    messages = [
        {
            "role": "system",
            "content": "You are simulating a U.S. persona in civic deliberation. End your answer with </final>."
        },
        {
            "role": "user",
            "content": (
                "PERSONA_TRAITS:\n" + json.dumps(traits, ensure_ascii=False) +
                "\n\nBIO:\n" + persona["biography"] +
                "\n\nTASK:\n"
                f"Topic: \"{topic}\"\n"
                "Instruction: In ≤180 tokens, write a clear, opinionated, first-person response consistent with the persona's values, tone, and taboos.\n"
                "Style: conversational, 1–2 concrete reasons, 1 concession if applicable.\n"
                "Output only the response text, then '</final>'."
            )
        }
    ]
    return messages

def build_judge_messages(persona: Dict[str, Any], candidate_text: str) -> List[Dict[str, str]]:
    traits = {
        "persona_id": persona["persona_id"],
        "demographics": persona["demographics"],
        "values_axes": persona["values_axes"],
        "style": persona["style"],
        "one_liner": persona["one_liner"],
    }
    rubric = (
        "RUBRIC (0–1 total):\n"
        "- Persona fidelity (0.0–0.4): values/style/taboos match?\n"
        "- Argument clarity (0.0–0.3): specific, on-topic, ≤180 tokens?\n"
        "- Constructiveness (0.0–0.2): acknowledges counterpoint, civil tone?\n"
        "- Local realism (0.0–0.1): lived/context cues appropriate?\n"
    )
    return [
        {"role": "system", "content": "You are a strict rubric judge. Output JSON only. End with </final>."},
        {"role": "user", "content": (
            "PERSONA_TRAITS:\n" + json.dumps(traits, ensure_ascii=False) +
            "\n\n" + rubric +
            "\nCANDIDATE:\n" + candidate_text +
            "\n\nReturn JSON: {\"score\": float, \"reason\": \"<≤15 words>\"} </final>"
        )}
    ]

# ----------------- API helpers ------------------

def chat_completion(model: str, messages: List[Dict[str, str]], temperature: float, top_p: float,
                    max_tokens: int, stop: List[str]) -> Dict[str, Any]:
    payload = {
        "model": model,
        "temperature": temperature,
        "top_p": top_p,
        "max_tokens": max_tokens,
        "stop": stop,
        "messages": messages
    }
    t0 = time.time()
    r = requests.post(CHAT_URL, json=payload, timeout=TIMEOUT_S)
    t1 = time.time()
    r.raise_for_status()
    data = r.json()
    content = data["choices"][0]["message"]["content"]
    usage = data.get("usage", {})
    return {"content": content, "usage": usage, "latency_s": t1 - t0, "raw": data}

def strip_final(text: str) -> str:
    return text.replace("</final>", "").strip()

def safe_parse_json(s: str) -> Dict[str, Any]:
    try:
        return json.loads(strip_final(s))
    except Exception:
        # Try to locate a JSON blob
        m = re.search(r"\{.*\}", s, flags=re.S)
        if m:
            try:
                return json.loads(m.group(0))
            except Exception:
                return {"score": None, "reason": "parse_error"}
        return {"score": None, "reason": "parse_error"}

# ----------------- Runner ------------------

def main():
    run_id = datetime.utcnow().strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:6]
    print(f"[eval] run_id={run_id}")
    os.makedirs("eval_outputs", exist_ok=True)

    # 1) Discover models
    try:
        all_models = list_models()
    except Exception as e:
        print(f"[error] Could not list models at {MODELS_URL}: {e}")
        return

    gens = dedupe(GEN_MODEL_IDS)
    judges = dedupe(JUDGE_MODEL_IDS)

    missing_gens = [m for m in gens if m not in all_models]
    missing_judges = [m for m in judges if m not in all_models]
    if missing_gens:
        print(f"[warn] Requested GEN models not present in list: {missing_gens}")
    if missing_judges:
        print(f"[warn] Requested JUDGE models not present in list: {missing_judges}")

    print(f"[models] GEN  ({len(gens)}): {gens}")
    print(f"[models] JUDGE({len(judges)}): {judges}")

    results = {
        "run_id": run_id,
        "lm_studio_url": LM_STUDIO_URL,
        "timestamp_utc": datetime.utcnow().isoformat() + "Z",
        "personas": [p["persona_id"] for p in PERSONAS],
        "topics": TOPICS,
        "gens": gens,
        "judges": judges,
        "k": K,
        "records": []  # list of dicts per candidate per judge
    }

    # 2) Matrix
    for gen_model in gens:
        candidate_records: List[Dict[str, Any]] = []

        print(f"[load] Loading generator '{gen_model}'")
        if not load_model(gen_model):
            print(f"[skip] Could not load generator '{gen_model}', skipping")
            continue

        try:
            for persona in PERSONAS:
                for topic in TOPICS:
                    for i in range(K):
                        try:
                            gen_resp = chat_completion(
                                model=gen_model,
                                messages=build_generator_messages(persona, topic),
                                temperature=GEN_TEMPERATURE,
                                top_p=GEN_TOP_P,
                                max_tokens=GEN_MAX_TOKENS,
                                stop=["</final>"]
                            )
                            cand_text = strip_final(gen_resp["content"])
                        except Exception as e:
                            cand_text = f"[GEN_ERROR] {e}"
                            gen_resp = {"latency_s": None, "usage": {}, "raw": {}}

                        candidate_records.append({
                            "persona": persona,
                            "persona_id": persona["persona_id"],
                            "topic": topic,
                            "candidate_index": i + 1,
                            "candidate_text": cand_text,
                            "gen_meta": gen_resp,
                        })
        finally:
            print(f"[unload] Releasing generator '{gen_model}'")
            unload_model(gen_model)

        if not candidate_records:
            print(f"[warn] No candidates generated for '{gen_model}', continuing")
            continue

        for judge_model in judges:
            print(f"[load] Loading judge '{judge_model}' for gen '{gen_model}'")
            if not load_model(judge_model):
                print(f"[skip] Could not load judge '{judge_model}', skipping")
                continue

            try:
                for cand in candidate_records:
                    persona = cand["persona"]
                    try:
                        judge_resp = chat_completion(
                            model=judge_model,
                            messages=build_judge_messages(persona, cand["candidate_text"]),
                            temperature=JUDGE_TEMPERATURE,
                            top_p=JUDGE_TOP_P,
                            max_tokens=JUDGE_MAX_TOKENS,
                            stop=["</final>"]
                        )
                        parsed = safe_parse_json(judge_resp["content"])
                        score = parsed.get("score")
                        reason = parsed.get("reason")
                    except Exception as e:
                        judge_resp = {"latency_s": None, "usage": {}, "raw": {}}
                        score = None
                        reason = f"JUDGE_ERROR: {e}"

                    rec = {
                        "gen_model": gen_model,
                        "judge_model": judge_model,
                        "persona_id": cand["persona_id"],
                        "topic": cand["topic"],
                        "candidate_index": cand["candidate_index"],
                        "candidate_text": cand["candidate_text"],
                        "gen_latency_s": cand["gen_meta"].get("latency_s"),
                        "judge_latency_s": judge_resp.get("latency_s"),
                        "score": score,
                        "reason": reason
                    }
                    results["records"].append(rec)
                    print(f"[scored] gen={gen_model} judge={judge_model} persona={cand['persona_id']} "
                          f"topic='{cand['topic'][:28]}...' k={cand['candidate_index']} score={score} ({reason})")
            finally:
                print(f"[unload] Releasing judge '{judge_model}'")
                unload_model(judge_model)

    # 3) Save raw results
    results_path = os.path.join("eval_outputs", f"results_{run_id}.json")
    with open(results_path, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    print(f"[saved] {results_path}")

    # 4) Build summary report (markdown)
    report_md = build_report(results)
    report_path = os.path.join("eval_outputs", f"report_{run_id}.md")
    with open(report_path, "w", encoding="utf-8") as f:
        f.write(report_md)
    print(f"[saved] {report_path}\n")
    print("-------- report.md (preview) --------")
    print(report_md[:2000])

def build_report(results: Dict[str, Any]) -> str:
    recs = results["records"]
    # Aggregate by (gen, judge)
    by_pair: Dict[Tuple[str, str], List[Dict[str, Any]]] = {}
    for r in recs:
        key = (r["gen_model"], r["judge_model"])
        by_pair.setdefault(key, []).append(r)

    def safe_avg(xs):
        xs = [x for x in xs if isinstance(x, (int, float))]
        return round(sum(xs) / len(xs), 4) if xs else None

    lines = []
    lines.append(f"# LM Studio Persona Eval — {results['run_id']}")
    lines.append(f"- LM Studio: `{results['lm_studio_url']}`")
    lines.append(f"- Personas: {', '.join(results['personas'])}")
    lines.append(f"- Topics: {len(results['topics'])} × K={results['k']} candidates each")
    lines.append(f"- GEN models: {', '.join(results['gens']) or 'N/A'}")
    lines.append(f"- JUDGE models: {', '.join(results['judges']) or 'N/A'}")
    lines.append("")
    lines.append("## Pair Summary (mean score, mean latencies)")
    lines.append("| GEN | JUDGE | n | mean_score | mean_gen_ms | mean_judge_ms |")
    lines.append("|---|---|---:|---:|---:|---:|")
    pair_stats = []
    for (gen, judge), L in sorted(by_pair.items()):
        scores = [r["score"] for r in L if isinstance(r["score"], (int, float))]
        gen_ms = [r["gen_latency_s"]*1000 for r in L if isinstance(r["gen_latency_s"], (int, float))]
        judge_ms = [r["judge_latency_s"]*1000 for r in L if isinstance(r["judge_latency_s"], (int, float))]
        lines.append(f"| {gen} | {judge} | {len(L)} | "
                     f"{(round(sum(scores)/len(scores), 4) if scores else '—')} | "
                     f"{(round(sum(gen_ms)/len(gen_ms), 1) if gen_ms else '—')} | "
                     f"{(round(sum(judge_ms)/len(judge_ms), 1) if judge_ms else '—')} |")
        pair_stats.append(((gen, judge), scores, gen_ms, judge_ms))

    # Best candidates per persona/topic by consensus (avg across all judges)
    lines.append("")
    lines.append("## Best Candidates by Persona/Topic (avg score across judges)")
    lines.append("| Persona | Topic | GEN | k | avg_score | snippet |")
    lines.append("|---|---|---|---:|---:|---|")

    # Group by (persona, topic, gen, candidate_index)
    group = {}
    for r in recs:
        key = (r["persona_id"], r["topic"], r["gen_model"], r["candidate_index"])
        group.setdefault(key, []).append(r)

    # Compute avg per group
    winners = []
    for key, L in group.items():
        scores = [x["score"] for x in L if isinstance(x["score"], (int, float))]
        if not scores:
            continue
        avg_score = sum(scores)/len(scores)
        sample_text = next((x["candidate_text"] for x in L if x["candidate_text"]), "")[:140].replace("\n", " ")
        winners.append((avg_score, key, sample_text))
    winners.sort(reverse=True)
    top_n = min(12, len(winners))
    for i in range(top_n):
        avg, (pid, topic, gen, kidx), snippet = winners[i]
        lines.append(f"| {pid} | {topic} | {gen} | {kidx} | {round(avg,4)} | {snippet} |")

    # Diagnostics
    lines.append("")
    lines.append("## Diagnostics & Notes")
    total = len(recs)
    parse_errors = sum(1 for r in recs if r["score"] is None)
    lines.append(f"- Total scored items: **{total}**; parse errors: **{parse_errors}**")
    # Judge agreement: std dev of scores across judges per candidate
    per_candidate = {}
    for r in recs:
        key = (r["persona_id"], r["topic"], r["gen_model"], r["candidate_index"])
        per_candidate.setdefault(key, []).append(r["score"])
    stdevs = []
    for key, scs in per_candidate.items():
        xs = [x for x in scs if isinstance(x, (int, float))]
        if len(xs) >= 2:
            stdevs.append(stats.pstdev(xs))
    if stdevs:
        lines.append(f"- Judge consensus (lower=better): mean σ across judges per candidate = **{round(sum(stdevs)/len(stdevs),4)}**")
    # Throughput
    gen_lat = [r["gen_latency_s"] for r in recs if isinstance(r["gen_latency_s"], (int, float))]
    judge_lat = [r["judge_latency_s"] for r in recs if isinstance(r["judge_latency_s"], (int, float))]
    if gen_lat:
        lines.append(f"- Mean GEN latency: **{round(1000*sum(gen_lat)/len(gen_lat),1)} ms**")
    if judge_lat:
        lines.append(f"- Mean JUDGE latency: **{round(1000*sum(judge_lat)/len(judge_lat),1)} ms**")

    # Pasteable meta-eval prompt
    lines.append("")
    lines.append("## Pasteable Meta-Eval Prompt (for GPT/UI)")
    lines.append("Copy this along with the table above to evaluate judges and overall quality:")
    meta = (
        "You are evaluating a persona-simulation pipeline.\n"
        "1) Look at 'Pair Summary' (mean scores/latencies) to pick the strongest GEN→JUDGE pairs.\n"
        "2) Inspect 'Best Candidates' snippets; check persona fidelity, clarity, constructiveness, and locality.\n"
        "3) Comment on judge consistency using the reported mean σ across judges.\n"
        "4) Recommend the best GEN model, best JUDGE model, and any prompt/length tweaks.\n"
        "5) Flag common failure modes (style drift, hedging, policy overreach, verbosity, boilerplate).\n"
        "Return concise bullets and a ranked list of pairs."
    )
    lines.append(f"```\n{meta}\n```")

    return "\n".join(lines)

if __name__ == "__main__":
    main()
