# GitOps CD Setup

## When to use
- First-time setup of the gitops CD pipeline
- Rotating the deploy key or webhook secret
- Debugging the `deploy-gitops` CI job

## Overview

The `deploy-gitops` job in CI automatically updates image digests in the `icook/homelab-gitops` repo after building images on `master`. This triggers Flux reconciliation and a rolling deployment.

```
Push to master
  → CI builds & pushes images to GHCR (~3-5 min)
  → deploy-gitops writes digests to homelab-gitops/main
  → GitHub webhook fires → Flux reconciles
  → Helm sees digest diff → rolling update (~30s)
Total: ~4-6 min from push to pods running
```

## Prerequisites

- SSH key generation tool (`ssh-keygen`)
- Admin access to both `icook/tiny-congress` and `icook/homelab-gitops` repos
- `kubectl` access to the cluster running Flux
- `sops` for decrypting the webhook token

## Step 1: Deploy key for gitops repo

Generate an Ed25519 SSH keypair:

```bash
ssh-keygen -t ed25519 -C "tiny-congress-ci" -f /tmp/gitops-deploy-key -N ""
```

Add the **public key** as a deploy key on `icook/homelab-gitops`:
1. Go to `icook/homelab-gitops` → Settings → Deploy keys → Add deploy key
2. Title: `tiny-congress-ci`
3. Paste contents of `/tmp/gitops-deploy-key.pub`
4. Check **Allow write access**

Add the **private key** as a repository secret on `icook/tiny-congress`:
1. Go to `icook/tiny-congress` → Settings → Secrets and variables → Actions
2. New repository secret
3. Name: `GITOPS_DEPLOY_KEY`
4. Paste contents of `/tmp/gitops-deploy-key`

Clean up the local key files:

```bash
rm /tmp/gitops-deploy-key /tmp/gitops-deploy-key.pub
```

## Step 2: Configure the Flux webhook

After the Flux receiver is deployed, get the webhook path:

```bash
kubectl -n flux-system get receiver github-receiver -o jsonpath='{.status.webhookPath}'
```

Get the webhook secret token:

```bash
sops --decrypt clusters/sauce/flux-system/webhook-token.sops.yaml
# Use the stringData.token field value
```

Configure webhooks on **both** repos (`icook/homelab-gitops` and `icook/tiny-congress`):
1. Go to repo → Settings → Webhooks → Add webhook
2. **Payload URL**: `https://flux-webhook.ibcook.com/<webhook-path>`
3. **Content type**: `application/json`
4. **Secret**: The token from the sops-decrypted file
5. **Events**: Just the `push` event

## Step 3: Verify the pipeline

Trigger a test run by pushing to `master` (or re-running the CI workflow).

Check the `deploy-gitops` job logs:
```bash
gh run view --repo icook/tiny-congress --log --job <job-id>
```

Verify the gitops repo was updated:
```bash
gh api repos/icook/homelab-gitops/commits/main --jq '.commit.message'
```

Check Flux reconciliation:
```bash
kubectl -n flux-system get kustomization -w
flux get helmrelease -n default
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `deploy-gitops` job not running | Not a push to `master` | Job only runs on `master` pushes |
| "Permission denied (publickey)" | Deploy key misconfigured | Verify `GITOPS_DEPLOY_KEY` secret matches the deploy key |
| Empty digest output | GHCR API path wrong | Check if `/orgs/` should be `/users/` for the account type |
| "No digest changes detected" | Images unchanged | Expected if no code changed; check GHCR for new tags |
| Flux not reconciling | Webhook not configured | Verify webhook shows recent successful deliveries |
| Pods not updating | Chart doesn't use digest | Verify `deployment.yaml` templates include `@digest` suffix |

## Verification checklist

- [ ] `GITOPS_DEPLOY_KEY` secret exists on `icook/tiny-congress`
- [ ] Deploy key with write access exists on `icook/homelab-gitops`
- [ ] Webhook configured on both repos with correct URL and secret
- [ ] `deploy-gitops` job completes successfully on a `master` push
- [ ] Gitops repo shows updated digest values after CI run
- [ ] Flux reconciles and pods show new image digests

## See also
- `.github/workflows/ci.yml` - The `deploy-gitops` job definition
- `kube/app/templates/deployment.yaml` - Helm templates with digest support
- `kube/app/values.yaml` - Default values including `digest: ""`
