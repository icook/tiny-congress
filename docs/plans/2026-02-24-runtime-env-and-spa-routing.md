# Runtime Env Config & SPA Routing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the frontend Docker image environment-agnostic by injecting API URL at container start, and fix SPA routing 404s.

**Architecture:** Replace build-time `import.meta.env.VITE_API_URL` with a runtime config pattern: a docker entrypoint script generates `/config.js` from env vars, which the app reads via `window.__TC_ENV__`. An nginx config adds `try_files` for SPA routing. Local dev (Vite) continues to use `import.meta.env` as a fallback.

**Tech Stack:** nginx, shell scripting, TypeScript, Vitest

---

### Task 1: Add nginx config for SPA routing

**Files:**
- Create: `web/nginx.conf`
- Modify: `web/Dockerfile:39-49`

**Step 1: Create `web/nginx.conf`**

```nginx
server {
    listen 80;
    root /usr/share/nginx/html;
    index index.html;

    # Runtime config must not be cached — it's regenerated on each container start
    location = /config.js {
        add_header Cache-Control "no-store, no-cache, must-revalidate";
    }

    location / {
        try_files $uri $uri/ /index.html;
    }
}
```

**Step 2: Update `web/Dockerfile` production stage**

Replace the production stage (lines 39–49) with:

```dockerfile
# Production stage
FROM nginx:alpine
WORKDIR /usr/share/nginx/html

# SPA routing + runtime config cache headers
COPY web/nginx.conf /etc/nginx/conf.d/default.conf

# Copy static assets from builder
COPY --from=builder /app/dist/ ./

# Entrypoint generates /config.js from env vars, then starts nginx
COPY web/docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

EXPOSE 80
ENTRYPOINT ["/docker-entrypoint.sh"]
```

Note: `docker-entrypoint.sh` is created in Task 3. These two tasks are committed together.

**Step 3: Commit**

No commit yet — depends on Task 3 (entrypoint script). Will commit together in Task 3.

---

### Task 2: Create runtime config module with tests

**Files:**
- Create: `web/src/config.ts`
- Create: `web/src/config.test.ts`

**Step 1: Write the failing test**

Create `web/src/config.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

describe('getApiBaseUrl', () => {
  const ORIGINAL_TC_ENV = window.__TC_ENV__;

  beforeEach(() => {
    // Clear any runtime config between tests
    delete window.__TC_ENV__;
  });

  afterEach(() => {
    window.__TC_ENV__ = ORIGINAL_TC_ENV;
    vi.unstubAllEnvs();
  });

  test('returns runtime config value when window.__TC_ENV__ is set', async () => {
    window.__TC_ENV__ = { VITE_API_URL: 'https://api.prod.example.com' };
    // Re-import to get fresh evaluation
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://api.prod.example.com');
  });

  test('falls back to import.meta.env.VITE_API_URL when no runtime config', async () => {
    vi.stubEnv('VITE_API_URL', 'https://api.staging.example.com');
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://api.staging.example.com');
  });

  test('falls back to localhost when neither runtime config nor env var is set', async () => {
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('http://localhost:8080');
  });

  test('runtime config takes precedence over import.meta.env', async () => {
    window.__TC_ENV__ = { VITE_API_URL: 'https://runtime.example.com' };
    vi.stubEnv('VITE_API_URL', 'https://buildtime.example.com');
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://runtime.example.com');
  });

  test('skips empty string in runtime config', async () => {
    window.__TC_ENV__ = { VITE_API_URL: '' };
    vi.stubEnv('VITE_API_URL', 'https://buildtime.example.com');
    const { getApiBaseUrl } = await import('./config');
    expect(getApiBaseUrl()).toBe('https://buildtime.example.com');
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd web && yarn vitest run src/config.test.ts`
Expected: FAIL — `./config` module does not exist.

**Step 3: Write the implementation**

Create `web/src/config.ts`:

```ts
/**
 * Runtime environment configuration.
 *
 * In production, docker-entrypoint.sh generates /config.js which sets
 * window.__TC_ENV__ from container environment variables.
 * In local dev, Vite's import.meta.env is used as a fallback.
 */

interface RuntimeConfig {
  VITE_API_URL?: string;
}

declare global {
  interface Window {
    __TC_ENV__?: RuntimeConfig;
  }
}

export function getApiBaseUrl(): string {
  return window.__TC_ENV__?.VITE_API_URL
    || (import.meta.env.VITE_API_URL as string | undefined)
    || 'http://localhost:8080';
}
```

Note: Using `||` (not `??`) so that empty strings fall through to the next source, which is important since the Helm default for `frontend.apiUrl` is `""`.

**Step 4: Run tests to verify they pass**

Run: `cd web && yarn vitest run src/config.test.ts`
Expected: All 5 tests PASS.

**Step 5: Commit**

```bash
git add web/src/config.ts web/src/config.test.ts
git commit -m "feat(web): add runtime config module for environment-agnostic builds

Reads API base URL from window.__TC_ENV__ (set by docker-entrypoint.sh at
container start), falling back to import.meta.env for local Vite dev."
```

---

### Task 3: Create docker entrypoint script

**Files:**
- Create: `web/docker-entrypoint.sh`

**Step 1: Create `web/docker-entrypoint.sh`**

```bash
#!/bin/sh
set -eu

# Generate runtime config from environment variables.
# This runs at container start so the same image works in any environment.
cat > /usr/share/nginx/html/config.js <<EOF
window.__TC_ENV__ = {
  VITE_API_URL: "${VITE_API_URL:?VITE_API_URL must be set}"
};
EOF

exec nginx -g 'daemon off;'
```

Uses `${VAR:?message}` per project design principle: "Fail loud over silent incorrectness."

**Step 2: Add the `<script>` tag to `web/index.html`**

Add before the main app script:

```html
<script src="/config.js"></script>
```

The full `<body>` becomes:
```html
<body>
    <div id="root"></div>
    <script src="/config.js"></script>
    <script type="module" src="/src/main.tsx"></script>
</body>
```

In local Vite dev, `/config.js` will 404 silently (non-module scripts don't block), and the app falls back to `import.meta.env`. This is fine — no stub needed.

**Step 3: Commit with the Dockerfile changes from Task 1**

```bash
git add web/nginx.conf web/docker-entrypoint.sh web/Dockerfile web/index.html
git commit -m "feat(web): add SPA routing and runtime env injection

- nginx.conf: try_files for SPA routing, no-cache on /config.js
- docker-entrypoint.sh: generates config.js from env vars at container start
- Dockerfile: uses entrypoint + custom nginx config
- index.html: loads /config.js before app bundle"
```

---

### Task 4: Wire API clients to use runtime config

**Files:**
- Modify: `web/src/api/graphqlClient.ts:6-7`
- Modify: `web/src/features/identity/api/client.ts:6-8`

**Step 1: Update `web/src/api/graphqlClient.ts`**

Replace lines 6-7:
```ts
const API_BASE_URL: string =
  (import.meta.env.VITE_API_URL as string | undefined) ?? 'http://localhost:8080';
```

With:
```ts
import { getApiBaseUrl } from '../config';
```

And update `getGraphqlUrl` to call `getApiBaseUrl()`:
```ts
export function getGraphqlUrl(): string {
  return `${getApiBaseUrl()}/graphql`;
}
```

**Step 2: Update `web/src/features/identity/api/client.ts`**

Replace lines 7-8:
```ts
const API_BASE_URL: string =
  (import.meta.env.VITE_API_URL as string | undefined) ?? 'http://localhost:8080';
```

With an import and inline usage:
```ts
import { getApiBaseUrl } from '../../../config';
```

And update `fetchJson` to use it:
```ts
async function fetchJson<T>(path: string, options?: RequestInit): Promise<T> {
  const url = `${getApiBaseUrl()}${path}`;
```

**Step 3: Run existing tests to verify nothing breaks**

Run: `cd web && yarn vitest run`
Expected: All tests pass (including `client.test.ts` — it mocks `fetch` and doesn't care about the URL source).

**Step 4: Run linting**

Run: `just lint-frontend`
Expected: PASS.

**Step 5: Commit**

```bash
git add web/src/api/graphqlClient.ts web/src/features/identity/api/client.ts
git commit -m "refactor(web): use runtime config for API base URL

Both graphqlClient and identity/api/client now read the API URL from
getApiBaseUrl() instead of inlining import.meta.env.VITE_API_URL."
```

---

### Task 5: Update Helm deployment to remove ineffective env var pattern

**Files:**
- Modify: `kube/app/templates/deployment.yaml:129-131`

The `VITE_API_URL` env var on the frontend container was previously ineffective (set at runtime but baked at build time). Now the entrypoint reads it at startup, so the Helm chart works as intended — **no changes needed to the template**.

However, update `kube/app/values.yaml` to document the changed behavior:

**Step 1: Update the `apiUrl` comment in `kube/app/values.yaml`**

Replace:
```yaml
  # Override API base URL (defaults to the API service DNS name).
  apiUrl: ""
```

With:
```yaml
  # API base URL injected at container start via docker-entrypoint.sh.
  # Defaults to the API service's internal DNS name if empty.
  apiUrl: ""
```

**Step 2: Commit**

```bash
git add kube/app/values.yaml
git commit -m "docs(kube): clarify frontend.apiUrl is now runtime-injected"
```

---

### Task 6: Verify the full build

**Step 1: Run all frontend tests**

Run: `cd web && yarn vitest run`
Expected: All tests pass.

**Step 2: Run frontend linting and type checking**

Run: `just lint-frontend && just typecheck`
Expected: PASS.

**Step 3: Verify Docker build succeeds**

Run: `docker build -f web/Dockerfile -t tc-ui-test .`
Expected: Build completes successfully.

**Step 4: Smoke-test the container**

Run:
```bash
docker run --rm -d -p 8888:80 -e VITE_API_URL=http://test-api:8080 --name tc-ui-smoke tc-ui-test
sleep 1
# SPA routing: sub-path should return index.html (200), not 404
curl -s -o /dev/null -w '%{http_code}' http://localhost:8888/some/deep/route
# config.js should contain the injected URL
curl -s http://localhost:8888/config.js
docker stop tc-ui-smoke
```
Expected:
- Sub-path returns `200`
- `config.js` contains `VITE_API_URL: "http://test-api:8080"`

**Step 5: Verify fail-loud behavior**

Run:
```bash
docker run --rm -e UNRELATED=foo tc-ui-test 2>&1 || true
```
Expected: Container exits with error message `VITE_API_URL must be set`.
