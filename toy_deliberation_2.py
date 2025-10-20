# TinyCongress — KISS multi‑phase deliberation demo (Jupyter-friendly)
# Phases:
#   1) Persona generation (diverse ideology axes)
#   2) Topic + disagreement levers
#   3) Brainstorm (sequential, one-sentence replies)
#   4) Triage (canonical propositions; diversity-first)
#   5) Stance labeling ([-2..+2], rubric & rarity rule)
#   6) Clustering + maps (tables + charts)
#   7) Focused deliberation (LLM-bridging statements)
#   8) Reflection & decision (rank bridging & polarizing)

import json, time, random, re, textwrap
from typing import List, Dict, Any, Tuple, Optional
import requests
import numpy as np
import pandas as pd
from sklearn.metrics.pairwise import cosine_similarity, cosine_distances
from sklearn.cluster import KMeans, AgglomerativeClustering
from sklearn.decomposition import PCA
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.metrics import silhouette_score
import matplotlib.pyplot as plt

# --------------------------- Configuration ---------------------------
LLM_ENDPOINT = "http://localhost:1234/v1/chat/completions"  # your local server
LLM_MODEL    = "llama-3.2-1b-instruct"                      # use this for ALL prompts
TEMPERATURE_GEN = 0.75                                      # for generative prompts (personas, replies, text)
TOP_P_GEN       = 0.9
TEMPERATURE_LABEL = 0.1                                     # for stance labeling (deterministic)
TOP_P_LABEL       = 0.9

RANDOM_SEED   = 42
N_PERSONAS    = 12
K_PROPS       = 12
N_CLUSTERS    = 2
REQUEST_TIMEOUT = 60

random.seed(RANDOM_SEED)
np.random.seed(RANDOM_SEED)

# --------------------------- LLM Client ------------------------------
def call_llm(messages: List[Dict[str, str]], temperature: float, top_p: float) -> str:
    """Call the OpenAI-compatible Chat Completions API (single endpoint only)."""
    payload = {
        "model": LLM_MODEL,
        "messages": messages,
        "temperature": float(temperature),
        "top_p": float(top_p),
        "max_tokens": -1,
        "stream": False,
    }
    resp = requests.post(
        LLM_ENDPOINT,
        headers={"Content-Type": "application/json"},
        data=json.dumps(payload),
        timeout=REQUEST_TIMEOUT,
    )
    resp.raise_for_status()
    data = resp.json()
    content = data["choices"][0]["message"]["content"]
    return content

def parse_json_strict(content: str) -> Any:
    """Parse JSON; try fenced blocks and raw_decode if needed."""
    if content is None:
        raise ValueError("LLM returned None")
    txt = content.strip()
    # strip accidental <think> tags
    txt = re.sub(r"</?think>", "", txt, flags=re.IGNORECASE).strip()
    # 1) direct
    try:
        return json.loads(txt)
    except Exception:
        pass
    # 2) fenced code
    m = re.search(r"```(?:json)?\s*(.*?)\s*```", txt, flags=re.DOTALL|re.IGNORECASE)
    if m:
        return json.loads(m.group(1))
    # 3) raw decode from first { or [
    dec = json.JSONDecoder()
    for match in re.finditer(r"[\[{]", txt):
        try:
            obj, _ = dec.raw_decode(txt[match.start():])
            return obj
        except Exception:
            continue
    raise ValueError("Could not parse JSON")

def llm_json(messages: List[Dict[str, str]], temperature: float, top_p: float) -> Any:
    """Call LLM and parse strict JSON; one retry with 'STRICT JSON only'."""
    out = call_llm(messages, temperature, top_p)
    try:
        return parse_json_strict(out)
    except Exception:
        retry = messages + [{"role":"user","content":"Respond again in STRICT JSON only, no prose."}]
        out2 = call_llm(retry, temperature, top_p)
        return parse_json_strict(out2)

# --------------------------- Helpers ---------------------------------
def tfidf_embed(texts: List[str]) -> np.ndarray:
    if not texts:
        return np.zeros((0,0))
    v = TfidfVectorizer(min_df=1, ngram_range=(1,2))
    return v.fit_transform(texts).toarray()

def mmr_select(items: List[str], k: int, lambda_div: float=0.65) -> List[str]:
    """Maximal Marginal Relevance using TF-IDF embeddings (no external models)."""
    if not items: return []
    if k >= len(items): return items
    E = tfidf_embed(items)
    if E.size == 0: return items[:k]
    selected, candidates = [], list(range(len(items)))
    seed_idx = max(candidates, key=lambda i: len(items[i]))  # pick the longest as seed (specificity proxy)
    selected.append(seed_idx); candidates.remove(seed_idx)
    while len(selected) < k and candidates:
        sims_to_sel = cosine_similarity(E[candidates], E[selected]).max(axis=1)
        sims_global = cosine_similarity(E[candidates], E).mean(axis=1)
        mmr = lambda_div*(1-sims_to_sel) + (1-lambda_div)*(1-sims_global)
        pick_rel = candidates[int(np.argmax(mmr))]
        selected.append(pick_rel); candidates.remove(pick_rel)
    return [items[i] for i in selected]

def strip_think(x: str) -> str:
    return re.sub(r"</?think>", "", x or "", flags=re.IGNORECASE).strip()

# --------------------------- Phase 1: Personas -----------------------
def generate_personas(n=N_PERSONAS) -> List[Dict[str, Any]]:
    sys = {"role":"system","content":"You fabricate realistic civic personas. When asked for JSON, return STRICT JSON only."}
    user = {"role":"user","content":textwrap.dedent(f"""
    Generate {n} civic personas as STRICT JSON under "personas".
    Use a 2D ideology grid and cover all quadrants (≥1 each):
      - Economic: left | center | right
      - Social:   liberal | moderate | conservative
    Add two orthogonal axes (assign one value each):
      - Tech: skeptic | pragmatic | booster
      - Governance: localist | federalist | market-first
    Each persona must include:
      - name
      - background (1 sentence; job, place)
      - values (4–6)
      - expertise (array from: policy, tech, community, healthcare, law, economics, education, environment, security, faith, disability, rural_dev, transit_ops)
      - communication_style (blunt | diplomatic | data-driven | narrative | pugilistic | legalistic | populist)
      - priors (3 bullets that imply disagreement with another quadrant)
      - axes: {{economic, social, tech, governance}}
    Global constraints:
      - Include: 1 fiscal-hawk, 1 property-rights advocate, 1 tenant-rights activist, 1 civil-liberties maximalist,
                 1 public-safety hardliner, 1 tech-skeptic.
      - Avoid generic language; embed concrete policy priors.
    Return STRICT JSON only.
    """).strip()}
    data = llm_json([sys, user], TEMPERATURE_GEN, TOP_P_GEN)
    personas = data["personas"]
    # display
    pdf = pd.DataFrame([{
        "name":p["name"],
        "econ":p.get("axes",{}).get("economic"),
        "social":p.get("axes",{}).get("social"),
        "tech":p.get("axes",{}).get("tech"),
        "governance":p.get("axes",{}).get("governance"),
        "comm_style":p.get("communication_style"),
        "background":p.get("background")
    } for p in personas])
    display(pdf)
    return personas

# --------------------------- Phase 2: Topic --------------------------
def generate_topic_and_levers() -> Dict[str, Any]:
    sys = {"role":"system","content":"You are a neutral deliberation facilitator."}
    user = {"role":"user","content":textwrap.dedent("""
    Return STRICT JSON:
    {
      "topic": one of [congestion pricing, firearm background checks, short-term rentals, campus protest rules, fentanyl policy, zoning upzoning, police oversight boards],
      "phase_plan": ["brainstorm","triage","focused_deliberation","reflection_and_decision"],
      "prompt": "Neutral question forcing trade-offs (who pays, whose freedom limited, how enforced, success metric).",
      "disagreement_levers": ["cost allocation","liberty vs safety","equity vs efficiency","local vs federal authority","surveillance vs privacy"]
    }
    """).strip()}
    tpp = llm_json([sys, user], TEMPERATURE_GEN, TOP_P_GEN)
    display(pd.DataFrame([tpp]))
    return tpp

# --------------------------- Phase 3: Brainstorm ---------------------
def persona_one_liner(persona: Dict[str,Any], topic: str, prompt: str, levers: List[str], prior_snippets: List[str]) -> str:
    sys = {"role":"system","content":"Reply IN CHARACTER. EXACTLY one sentence, 18–26 words. No metadata, no XML, no <think>."}
    others = "\n".join(prior_snippets[-4:]) if prior_snippets else "(none so far)"
    user = {"role":"user","content":textwrap.dedent(f"""
    Persona JSON:
    {json.dumps(persona, ensure_ascii=False)}
    Recent replies (for diversity; do NOT imitate):
    {others}

    Topic: {topic}
    Prompt: {prompt}
    In your sentence, touch at least one lever: {", ".join(levers)}

    Constraints:
    - Exactly one sentence (18–26 words).
    - Expose a concrete trade‑off tied to your axes.
    - If your stance matches most prior replies, choose the closest justified dissent consistent with your priors.
    """).strip()}
    txt = call_llm([sys, user], TEMPERATURE_GEN, TOP_P_GEN)
    return strip_think(txt)

def run_brainstorm(personas: List[Dict[str,Any]], topic: str, prompt: str, levers: List[str]) -> Dict[str,str]:
    responses, prior = {}, []
    for p in personas:
        resp = persona_one_liner(p, topic, prompt, levers, prior)
        responses[p["name"]] = resp
        prior.append(f'{p["name"]}: {resp}')
    df = pd.DataFrame([{"name":k,"reply":v} for k,v in responses.items()])
    display(df)
    return responses

# --------------------------- Phase 4: Triage -------------------------
def extract_diverse_propositions(responses: Dict[str,str], k=K_PROPS) -> List[str]:
    sys = {"role":"system","content":"You turn short replies into canonical policy propositions."}
    joined = "\n".join([f"{k}: {v}" for k,v in responses.items()])
    user = {"role":"user","content":textwrap.dedent(f"""
    From these one-sentence replies:
    {joined}

    1) Propose ~{k*2} short, atomic, mutually non-overlapping propositions spanning opposed frames
       (liberty, safety, equity, efficiency, property-rights, tenant-rights).
    2) Then self‑prune to the most diverse {k} using a Maximal Marginal Relevance mindset (retain opposed frames).

    Return STRICT JSON:
    {{"propositions_all":[...], "propositions":[...]}}
    """).strip()}
    data = llm_json([sys, user], TEMPERATURE_GEN, TOP_P_GEN)
    props = data.get("propositions") or data.get("propositions_all") or []
    # Light dedup & MMR safety net
    props = [" ".join(p.split()) for p in props if p and p.strip()]
    props = list(dict.fromkeys(props))  # preserve order, dedup
    if len(props) > k:
        props = mmr_select(props, k)
    display(pd.DataFrame({"proposition":props}))
    return props

# --------------------------- Phase 5: Stance labeling ----------------
def label_stances_scalar(persona_name: str, persona_text: str, propositions: List[str]) -> Dict[str, Dict[str, Any]]:
    sys = {"role":"system","content":"You label stances strictly by the provided sentence. Return STRICT JSON only."}
    rubric = """
Scale (apply strictly from the persona's sentence):
  -2 strongly disagree: explicit rejection or opposing mechanism
  -1 somewhat disagree: clear reservations or prefer weaker/alternative version
   0 unsure/neutral: insufficient commitment in the sentence
  +1 somewhat agree: support with caveat/trade-off
  +2 strongly agree: explicit endorsement as stated

Rarity rule:
  - Use ±2 sparingly (≤ ceil(0.3 * N_props)).
  - Prefer 0 when the sentence does not commit clearly.
  - If violated, REVISE labels to satisfy the rule while staying faithful.
Fields per proposition:
  "stance" ∈ [-2,-1,0,1,2], "confidence" ∈ [0,1], "rationale": 3–6 words
"""
    user = {"role":"user","content":textwrap.dedent(f"""
    Persona response:
    {persona_name}: {persona_text}

    Label EACH proposition with the rubric and rarity rule.

    Propositions:
    {json.dumps(propositions, ensure_ascii=False)}

    Return STRICT JSON:
    {{"labels": {{"<prop>": {{"stance": INT, "confidence": FLOAT, "rationale": "..."}} , ... }}, "extremes_used": INT}}
    """).strip()}
    data = llm_json([sys, user], TEMPERATURE_LABEL, TOP_P_LABEL)
    return data["labels"]

def build_matrix(personas: List[str], props: List[str], labels_by_persona: Dict[str,Dict[str,Dict[str,Any]]]) -> np.ndarray:
    M = np.zeros((len(personas), len(props)), dtype=float)
    for i, name in enumerate(personas):
        labs = labels_by_persona[name]
        for j, p in enumerate(props):
            M[i,j] = float(labs.get(p,{}).get("stance",0))
    return M

# --------------------------- Phase 6: Clustering & maps --------------
def cluster_personas(M: np.ndarray, k=N_CLUSTERS, seed=RANDOM_SEED) -> Tuple[np.ndarray, float]:
    if M.size == 0:
        return np.zeros(0,dtype=int), -1.0
    # Try KMeans, fall back to Agglomerative on cosine distances
    try:
        km = KMeans(n_clusters=min(k, len(M)), random_state=seed, n_init="auto")
        labels = km.fit_predict(M)
        sil = silhouette_score(M, labels, metric="cosine") if len(set(labels))>1 else -1.0
        return labels, sil
    except Exception:
        D = cosine_distances(M + 1e-9)
        agg = AgglomerativeClustering(n_clusters=min(k, len(M)), affinity="precomputed", linkage="average")
        labels = agg.fit_predict(D)
        sil = silhouette_score(M, labels, metric="cosine") if len(set(labels))>1 else -1.0
        return labels, sil

def bridging_scores(M: np.ndarray, labels: np.ndarray) -> np.ndarray:
    n_personas, n_props = M.shape
    uniq = np.unique(labels)
    if len(uniq) < 2:
        return np.ones(n_props, dtype=float)
    weights = {c: np.mean(labels==c) for c in uniq}
    scores = np.zeros(n_props, dtype=float)
    for j in range(n_props):
        means = {c: M[labels==c, j].mean() for c in uniq}
        overall = sum(weights[c]*means[c] for c in uniq)
        penalty = sum(weights[c]*abs(means[c]-overall) for c in uniq)
        scores[j] = max(0.0, 1.0 - (penalty/2.0))   # max gap is 2 (−2 vs +2)
    return scores

def plot_pca_scatter(M: np.ndarray, labels: np.ndarray, names: List[str]):
    if M.shape[0] < 2: return
    X = M if M.shape[1] < 2 else PCA(n_components=2, random_state=RANDOM_SEED).fit_transform(M)
    plt.figure()
    for c in sorted(set(labels)):
        idx = np.where(labels==c)[0]
        plt.scatter(X[idx,0], X[idx,1], label=f"Cluster {c}")
    for i, n in enumerate(names):
        plt.text(X[i,0], X[i,1], n, fontsize=8)
    plt.title("Participant map (PCA over stance vectors)")
    plt.legend()
    plt.show()

def plot_bridging(bridge: np.ndarray, props: List[str], top_n=10, title="Bridging scores (higher = cross-cluster overlap)"):
    order = np.argsort(-bridge)[:min(top_n, len(bridge))]
    plt.figure()
    plt.bar(range(len(order)), bridge[order])
    plt.xticks(range(len(order)), [f"P{int(i)+1}" for i in order], rotation=0)
    plt.title(title)
    plt.show()
    df = pd.DataFrame({"prop_index":[int(i)+1 for i in order], "bridging_score":bridge[order], "proposition":[props[i] for i in order]})
    display(df)

def plot_polarizing(bridge: np.ndarray, props: List[str], top_n=10, title="Polarization gaps (higher = more split)"):
    gap = 1.0 - bridge
    order = np.argsort(-gap)[:min(top_n, len(gap))]
    plt.figure()
    plt.bar(range(len(order)), gap[order])
    plt.xticks(range(len(order)), [f"P{int(i)+1}" for i in order], rotation=0)
    plt.title(title)
    plt.show()
    df = pd.DataFrame({"prop_index":[int(i)+1 for i in order], "gap":gap[order], "proposition":[props[i] for i in order]})
    display(df)

# --------------------------- Phase 7: Focused deliberation -----------
def propose_bridging_statements(props: List[str], bridge_scores: List[float], top_k:int=5) -> List[str]:
    # Provide top bridging + top polarizing to LLM and ask for bridging statements
    order_bridge = np.argsort(-np.array(bridge_scores))[:min(top_k, len(props))]
    order_polar  = np.argsort( np.array(bridge_scores))[:min(top_k, len(props))]
    sys = {"role":"system","content":"You are a neutral facilitator crafting bridging statements that both sides might endorse."}
    user = {"role":"user","content":textwrap.dedent(f"""
    Given these propositions (index, text) and their bridging scores (0..1, higher is more cross-cluster overlap):

    TOP BRIDGING:
    {json.dumps([{ "index":int(i)+1, "prop":props[i], "bridging":float(bridge_scores[i]) } for i in order_bridge], ensure_ascii=False, indent=2)}

    MOST POLARIZING:
    {json.dumps([{ "index":int(i)+1, "prop":props[i], "bridging":float(bridge_scores[i]) } for i in order_polar], ensure_ascii=False, indent=2)}

    Task:
    - Propose 5 concise BRIDGING STATEMENTS (one sentence each) that preserve core policy content but adjust framing to improve cross-cluster support.
    - Vary frames (liberty, safety, equity, efficiency, property-rights, tenant-rights).
    Return STRICT JSON: {{"bridges":[ "...", "...", "...", "...", "..." ]}}
    """).strip()}
    data = llm_json([sys, user], TEMPERATURE_GEN, TOP_P_GEN)
    bridges = [strip_think(s) for s in data.get("bridges", []) if s and s.strip()]
    display(pd.DataFrame({"bridging_statement":bridges}))
    return bridges

# --------------------------- Phase 8: Reflection & decision ----------
def summarize_and_decide(M: np.ndarray, labels: np.ndarray, props: List[str], bridge: np.ndarray):
    # Simple reflection: report cluster means and suggest top 3 bridging props as "recommendations"
    uniq = sorted(set(labels))
    centroids = [M[labels==c].mean(axis=0) for c in uniq]
    cent_df = pd.DataFrame(centroids, columns=[f"P{j+1}" for j in range(len(props))])
    cent_df.insert(0, "cluster", uniq)
    display(cent_df)
    # Recommendations: top-3 bridging props
    order = list(np.argsort(-bridge)[:min(3, len(bridge))])
    recs = [{"rank":i+1,"prop_index":int(j)+1,"proposition":props[j],"bridging_score":float(bridge[j])} for i,j in enumerate(order)]
    display(pd.DataFrame(recs))

# --------------------------- Orchestration ---------------------------
def run_pipeline():
    print("=== Phase 1: Personas ===")
    personas = generate_personas(N_PERSONAS)

    print("\n=== Phase 2: Topic & Levers ===")
    tpp = generate_topic_and_levers()
    topic, prompt = tpp["topic"], tpp["prompt"]
    levers = tpp.get("disagreement_levers", [])

    print("\n=== Phase 3: Brainstorm (one-liners) ===")
    responses = run_brainstorm(personas, topic, prompt, levers)

    print("\n=== Phase 4: Triage (propositions) ===")
    props = extract_diverse_propositions(responses, K_PROPS)

    print("\n=== Phase 5: Stance labeling ([-2..+2]) ===")
    persona_names = [p["name"] for p in personas]
    labels_by_persona = {}
    for p in persona_names:
        labels_by_persona[p] = label_stances_scalar(p, responses[p], props)
    # Tabulate stance matrix
    M = build_matrix(persona_names, props, labels_by_persona)
    dfM = pd.DataFrame(M, index=persona_names, columns=[f"P{j+1}" for j in range(len(props))])
    display(dfM)

    print("\n=== Phase 6: Clustering & maps ===")
    labels, sil = cluster_personas(M, k=N_CLUSTERS, seed=RANDOM_SEED)
    dfC = pd.DataFrame({"name":persona_names, "cluster":labels})
    display(dfC.sort_values("cluster"))
    print(f"Silhouette (cosine): {sil:.3f}")
    plot_pca_scatter(M, labels, persona_names)

    bridge = bridging_scores(M, labels)
    plot_bridging(bridge, props, top_n=min(10,len(props)))
    plot_polarizing(bridge, props, top_n=min(10,len(props)))

    print("\n=== Phase 7: Focused deliberation (bridging statements) ===")
    bridges = propose_bridging_statements(props, bridge, top_k=5)

    print("\n=== Phase 8: Reflection & decision ===")
    summarize_and_decide(M, labels, props, bridge)
    print("\nDone.")

# --------------------------- Run ------------------------------------
run_pipeline()
