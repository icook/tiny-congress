# CD Setup (ArgoCD)

## When to use
- First-time setup of the ArgoCD CI deploy pipeline
- Rotating the ArgoCD CI token
- Debugging the `deploy-argocd` CI job

## Overview

The `deploy-argocd` job in CI sets image digests on the ArgoCD Application and triggers a sync after building images on `master`. No write access to `homelab-gitops` is needed.

```
Push to master
  → CI builds & pushes images to GHCR (~3-5 min)
  → deploy-argocd sets digests via ArgoCD API + syncs
  → ArgoCD rolling update (~30s)
Total: ~4-6 min from push to pods running
```

### ArgoCD config (in homelab-gitops)
- **CI account**: `accounts.ci: apiKey` (API-only, no UI login)
- **CI RBAC**: sync, get, override on `default/tiny-congress-demo` only
- **Application**: `tiny-congress-demo` in `default` project

## Prerequisites

- `argocd` CLI
- `gh` CLI authenticated with admin access to `icook/tiny-congress`
- Browser access to `argocd.ibcook.com` for SSO login

## LLM delegation

- **LLM**: Safe to delegate — no secrets involved
- **HUMAN**: Requires handling secret material — must be run by a human

## Step 1: Generate ArgoCD CI token (HUMAN)

Log in via SSO and generate an API token for the `ci` account:

```bash
argocd login argocd.ibcook.com --sso --grpc-web
argocd account generate-token --account ci --grpc-web
```

## Step 2: Set GHA secrets (HUMAN)

```bash
gh secret set ARGOCD_AUTH_TOKEN --repo icook/tiny-congress --body "<token>"
gh secret set ARGOCD_SERVER --repo icook/tiny-congress --body "argocd.ibcook.com"
```

## Step 3: Verify the pipeline (LLM)

Check that secrets exist (does not reveal values):

```bash
gh secret list --repo icook/tiny-congress | grep -E 'ARGOCD_(AUTH_TOKEN|SERVER)'
```

Watch for a CI run on the latest master push:

```bash
gh run list --repo icook/tiny-congress \
  --branch master --limit 3 \
  --json databaseId,status,conclusion,displayTitle
```

Check `deploy-argocd` job output from the most recent run:

```bash
RUN_ID=$(gh run list --repo icook/tiny-congress \
  --branch master --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$RUN_ID" --repo icook/tiny-congress --log \
  --job "$(gh run view "$RUN_ID" --repo icook/tiny-congress \
    --json jobs --jq '.jobs[] | select(.name == "Deploy via ArgoCD") | .databaseId')"
```

Check ArgoCD app status:

```bash
argocd app get tiny-congress-demo --grpc-web
# Should show Synced + Healthy with the new digest
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `deploy-argocd` job not running | Not a push to `master` | Job only runs on `master` pushes with deployable changes |
| "ARGOCD_AUTH_TOKEN not set" | Secret missing | Re-run Step 2 |
| ArgoCD permission denied | CI token expired or RBAC wrong | Regenerate token (Step 1); check RBAC in homelab-gitops |
| Empty digest output | GHCR API path wrong | Check if `/orgs/` should be `/users/` for the account type |
| `--grpc-web` errors | Cloudflare Tunnel config | Native gRPC not supported through Cloudflare Tunnel; `--grpc-web` is required |
| Pods not updating | Chart doesn't use digest | Verify templates include `@digest` suffix |

## Verification checklist

- [ ] `ARGOCD_AUTH_TOKEN` secret exists on `icook/tiny-congress` (LLM — existence only)
- [ ] `ARGOCD_SERVER` secret exists on `icook/tiny-congress` (LLM — existence only)
- [ ] `deploy-argocd` job completes on a `master` push (LLM)
- [ ] `argocd app get tiny-congress-demo` shows Synced + Healthy (LLM)

## See also
- `.github/workflows/ci.yml` — The `deploy-argocd` job definition
- `kube/app/templates/deployment.yaml` — Helm templates with digest support
- `kube/app/values.yaml` — Default values including `digest: ""`
- ArgoCD CI account config lives in `homelab-gitops` (see handoff doc)
