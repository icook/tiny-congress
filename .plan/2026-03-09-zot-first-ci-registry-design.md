# Zot-First CI Registry Design

**Date:** 2026-03-09
**Branch:** `feature/zot-first-ci-registry`
**Problem:** ARC CI jobs fail when egress to Docker Hub or GHCR is flaky — TLS timeouts, rate limits, DNS failures. Every job that pulls an image is a single point of failure on outbound internet.

## Decision

GHCR stays the canonical registry. Zot is a transparent accelerator for ARC runners. GHA-hosted runners are unchanged.

## Registry Routing

| | ARC (self-hosted) | GHA (hosted) |
|---|---|---|
| Build push target | zot (in-cluster) | GHCR |
| Pull source (scan, e2e, coverage, sqlx) | zot | GHCR |
| Layer cache | zot (already working) | GHCR (already working) |
| Base image pulls (docker.io) | zot pull-through | Docker Hub direct |
| Promote to GHCR | deploy-gitops job (master only) | not needed |

## Flow

1. **Build** → push to zot (`zot.zot.svc.cluster.local:5000/images/<image>:<sha>`)
2. **Test/scan** → pull from zot (in-cluster, no egress)
3. **All pass + master merge** → `crane copy` from zot → GHCR in deploy-gitops job
4. **deploy-gitops** → read digests from GHCR, update homelab-gitops as today

PR branch images never reach GHCR. They exist only in zot and are garbage collected.

## CI Workflow Changes

### `detect-changes` job
- New output: `image-registry`
  - ARC: `zot.zot.svc.cluster.local:5000/images`
  - GHA: `ghcr.io/icook/tiny-congress`

### `build-images` job
- Tag computation uses `image-registry` output instead of `${{ env.REGISTRY }}`
- On ARC: skip GHCR login, push to zot
- On GHA: push to GHCR as today
- Retag step operates within the same registry (zot or GHCR)

### `scan-images` job
- Trivy `image-ref` uses `image-registry` output
- On ARC: skip GHCR login, may need `TRIVY_INSECURE=true` for HTTP zot

### `rust-coverage` job
- `TEST_POSTGRES_IMAGE` uses `image-registry` output
- On ARC: skip GHCR login

### `sqlx-check` job
- `docker run` image ref uses `image-registry` output
- On ARC: skip GHCR login

### `e2e-tests` job
- `SKAFFOLD_DEFAULT_REPO` uses `image-registry` output
- Skaffold artifacts file uses `image-registry` output
- KinD containerd configured with zot mirror via `containerdConfigPatches`
- On ARC: skip GHCR login + imagePullSecrets (zot is unauthenticated)

### `deploy-gitops` job
- New step before digest lookup: `crane copy` each image from zot → GHCR
- Runs on `runner-small` (ARC on ARC, GHA on GHA)
- On GHA: skip promote (images already in GHCR)
- Then reads digests from GHCR as today

## KinD + Zot

KinD containerd gets a mirror config via kind cluster config:

```yaml
containerdConfigPatches:
  - |-
    [plugins."io.containerd.grpc.v1.cri".registry.mirrors."zot.zot.svc.cluster.local:5000"]
      endpoint = ["http://zot.zot.svc.cluster.local:5000"]
```

KinD's containerd resolves `zot.zot.svc.cluster.local` via the runner pod's DNS (CoreDNS). Falls back to `kind load docker-image` if DNS doesn't resolve.

## Infrastructure Prerequisites (homelab-gitops)

These land first, before CI changes:

1. **Zot**: Add sync config for `docker.io` and `ghcr.io` pull-through (on-demand). Create `/images` namespace.
2. **BuildKit** (`buildkitd.toml`): Add `[registry."docker.io"]` and `[registry."ghcr.io"]` mirrors → zot.
3. **DinD** (`daemon.json`): Add `registry-mirrors` for docker.io → zot. Add `insecure-registries` for zot.

## Rollback

Flip `USE_ARC_RUNNERS=false` repo variable → all CI routes to GHA → GHCR-direct path, no code change needed.

## Not In Scope

- Demo cluster pulling from zot (separate gitops change)
- Zot garbage collection / retention policy
- GHCR pull-through via zot for GHA runners
- Retry logic on promote step (crane copy is idempotent)
