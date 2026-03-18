# LLM + Search Cache Proxy Setup

## When to use

- Setting up LiteLLM as a caching proxy for the sim worker's LLM calls
- Adding Exa search caching via nginx to avoid duplicate search costs
- Deploying to the homelab (sauce cluster) alongside the TC demo

## Problem

The sim worker makes paid API calls to OpenRouter (LLM synthesis) and Exa (web search) every run. The same company+dimension query produces nearly identical results — paying twice is waste. A caching proxy eliminates duplicate costs across local dev, demo, and prod environments.

**Cost impact:** Without cache, a full 25-company seed costs ~$1.00. With cache, the first run costs $1.00 and all subsequent runs cost $0.00 until TTL expires.

## Architecture

```
sim binary
  ├── LLM calls ──→ LiteLLM Proxy (:4001) ──→ OpenRouter
  │                      ↕ disk cache
  └── Exa calls ──→ nginx cache   (:4002) ──→ api.exa.ai
                         ↕ file cache
```

Two separate services, both deployed as containers:
1. **LiteLLM Proxy** — OpenAI-compatible caching proxy for all LLM calls
2. **nginx cache** — generic HTTP reverse proxy cache for Exa API calls

## Part 1: LiteLLM Proxy

### What it does

Transparent proxy that speaks the OpenAI chat completions API. Caches request/response pairs to disk keyed on (model + messages + params). Returns cached response on exact match within TTL.

### Local setup

```bash
pip install 'litellm[proxy]'

# Create config
cat > litellm-config.yaml <<'EOF'
model_list:
  # DeepSeek V3.2 via OpenRouter (evidence synthesis)
  - model_name: deepseek/deepseek-v3.2
    litellm_params:
      model: openrouter/deepseek/deepseek-v3.2
      api_base: https://openrouter.ai/api/v1
      api_key: os.environ/OPENROUTER_API_KEY

  # Haiku via OpenRouter (fallback / alternative)
  - model_name: anthropic/claude-haiku-4-5
    litellm_params:
      model: openrouter/anthropic/claude-haiku-4-5
      api_base: https://openrouter.ai/api/v1
      api_key: os.environ/OPENROUTER_API_KEY

  # Sonnet via OpenRouter (battery testing / high quality)
  - model_name: anthropic/claude-sonnet-4-6
    litellm_params:
      model: openrouter/anthropic/claude-sonnet-4-6
      api_base: https://openrouter.ai/api/v1
      api_key: os.environ/OPENROUTER_API_KEY

litellm_settings:
  cache: true
  cache_params:
    type: disk
    disk_cache_dir: /data/litellm-cache
    ttl: 604800  # 7 days
EOF

# Run
OPENROUTER_API_KEY=$(pass show openrouter) \
  litellm --config litellm-config.yaml --port 4001
```

### Sim binary config change

Point the sim at LiteLLM instead of OpenRouter directly:

```bash
# Before (direct to OpenRouter):
SIM_OPENROUTER_API_KEY=sk-or-...

# After (through LiteLLM proxy):
# LiteLLM handles auth — the sim just needs any non-empty key
# and a different base URL. The actual OpenRouter key lives in
# LiteLLM's config.
```

**Code change needed in `service/src/sim/llm.rs`:** The OpenRouter base URL is currently hardcoded as `https://openrouter.ai/api/v1/chat/completions`. This needs to become configurable via `SIM_LLM_BASE_URL` (default: `https://openrouter.ai/api/v1`). Then:
- Local dev: `SIM_LLM_BASE_URL=http://localhost:4001`
- Demo cluster: `SIM_LLM_BASE_URL=http://litellm.tiny-congress-demo.svc.cluster.local:4001`

### Kubernetes deployment (sauce cluster)

Deploy as a sidecar or standalone Deployment in the `tiny-congress-demo` namespace.

**Helm values addition** (in `kube/environments/demo.yaml`):

```yaml
litellm:
  enabled: true
  port: 4001
  cacheDir: /data/litellm-cache
  cacheTtl: 604800
  # PVC for cache persistence across pod restarts
  persistence:
    enabled: true
    size: 1Gi
```

**Secrets:** The OpenRouter API key needs to be available to LiteLLM. Options:
1. Mount the existing SOPS-encrypted `sim.openrouterApiKey` as an env var in the LiteLLM pod
2. Create a separate SOPS secret for LiteLLM (cleaner separation)

**Docker image:** `ghcr.io/berriai/litellm:main-latest` (official, multi-arch)

**Resource requests:** LiteLLM is lightweight — 50m CPU / 128Mi memory is fine for demo scale.

### Verification

```bash
# Check cache is working (run same request twice)
curl http://localhost:4001/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "deepseek/deepseek-v3.2", "messages": [{"role": "user", "content": "hello"}]}'

# Second call should return instantly with X-Cache header
# Check cache directory has files
ls /data/litellm-cache/
```

## Part 2: Exa Search Cache (nginx)

### What it does

nginx reverse proxy that caches POST requests to Exa's API. Cache key is based on the full request body hash (same search query = same results within TTL).

### nginx config

```nginx
# exa-cache.conf
proxy_cache_path /data/exa-cache levels=1:2 keys_zone=exa:10m
                 max_size=100m inactive=7d use_temp_path=off;

server {
    listen 4002;

    location / {
        proxy_pass https://api.exa.ai;

        # Cache POST requests (not default nginx behavior)
        proxy_cache exa;
        proxy_cache_methods POST;
        proxy_cache_key "$request_uri|$request_body";
        proxy_cache_valid 200 7d;
        proxy_cache_use_stale error timeout updating;

        # Pass through auth headers
        proxy_set_header x-api-key $http_x_api_key;
        proxy_set_header Content-Type $http_content_type;

        # Cache status header for debugging
        add_header X-Cache-Status $upstream_cache_status;
    }
}
```

### Local setup

```bash
# Docker one-liner
docker run -d --name exa-cache \
  -p 4002:4002 \
  -v exa-cache-data:/data/exa-cache \
  -v $(pwd)/exa-cache.conf:/etc/nginx/conf.d/default.conf:ro \
  nginx:alpine
```

### Sim binary config change

Similar to LiteLLM — needs a configurable Exa base URL:

```bash
# Before (hardcoded in llm.rs):
# POST https://api.exa.ai/search

# After:
SIM_EXA_BASE_URL=http://localhost:4002  # through cache
```

**Code change needed:** Replace `https://api.exa.ai` in `exa_search()` with `config.exa_base_url` (default: `https://api.exa.ai`).

### Kubernetes deployment

Same pattern as LiteLLM — ConfigMap for nginx config, PVC for cache data, small Deployment.

**Note:** The nginx POST caching requires `proxy_cache_key` to include `$request_body`. For large request bodies, nginx buffers to disk which is fine for our ~200-byte Exa search payloads.

## Part 3: Code Changes Summary

These are the minimal code changes needed to make the sim binary cache-aware:

| File | Change |
|---|---|
| `service/src/sim/config.rs` | Add `llm_base_url: String` (default `https://openrouter.ai/api/v1`) and `exa_base_url: String` (default `https://api.exa.ai`) |
| `service/src/sim/llm.rs` | Replace hardcoded URLs in `generate_company_curation()`, `generate_company_evidence()`, `exa_search()`, and `generate_company_evidence_with_overrides()` with config values |
| `kube/app/templates/deployment.yaml` | Wire `SIM_LLM_BASE_URL` and `SIM_EXA_BASE_URL` from Helm values |
| `kube/environments/demo.yaml` | Add LiteLLM + nginx cache config |

## TTL Strategy

| Content type | TTL | Rationale |
|---|---|---|
| Exa search results | 7 days | Company news changes weekly-ish |
| LLM synthesis (evidence cards) | 7 days | Match search TTL — stale input = stale output |
| LLM curation (company list) | 30 days | S&P 500 composition changes rarely |

## Cost Model (with cache)

| Scenario | First run | Subsequent runs (within TTL) |
|---|---|---|
| Full 25-company seed | ~$1.00 | $0.00 |
| Single company re-research | ~$0.04 | $0.00 |
| Battery test (5 models, 1 company) | ~$0.20 | $0.00 for cached models |

## Alternatives Considered

- **GPTCache:** Python library, not a standalone proxy. Requires code integration.
- **Portkey Gateway:** Strong but requires Redis. Bundled Redis in Docker works but adds complexity.
- **Bifrost:** Single Go binary, great performance, but storage persistence unclear.
- **Custom Rust proxy:** ~200 lines with SQLite. Would unify LLM + Exa caching in one service. Worth building if the two-service approach (LiteLLM + nginx) proves annoying.
