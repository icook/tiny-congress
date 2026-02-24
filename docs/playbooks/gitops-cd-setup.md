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

- `gh` CLI authenticated with admin access to both repos
- `ssh-keygen` for key generation
- `kubectl` access to the cluster running Flux
- `sops` for decrypting the webhook token

## LLM delegation

Steps are marked to indicate who should run them:

- **LLM**: Safe to delegate — no secrets involved
- **HUMAN**: Requires handling secret material — must be run by a human

## Step 1: Generate deploy key (HUMAN)

Generate an Ed25519 SSH keypair. This produces secret material that must not be exposed to an LLM.

```bash
ssh-keygen -t ed25519 -C "tiny-congress-ci" -f /tmp/gitops-deploy-key -N ""
```

## Step 2: Add public deploy key to gitops repo (LLM)

The public key is not secret and can be added by an LLM:

```bash
gh repo deploy-key add /tmp/gitops-deploy-key.pub \
  --repo icook/homelab-gitops \
  --title "tiny-congress-ci" \
  --allow-write
```

Verify it was added:

```bash
gh repo deploy-key list --repo icook/homelab-gitops
```

## Step 3: Set deploy key secret on tiny-congress (HUMAN)

The private key is secret. Set it as a repository secret manually:

```bash
gh secret set GITOPS_DEPLOY_KEY \
  --repo icook/tiny-congress \
  < /tmp/gitops-deploy-key
```

Clean up the local key files:

```bash
rm /tmp/gitops-deploy-key /tmp/gitops-deploy-key.pub
```

## Step 4: Get Flux webhook path (LLM)

```bash
kubectl -n flux-system get receiver github-receiver \
  -o jsonpath='{.status.webhookPath}'
```

Save the output — it's needed for webhook configuration but is not secret on its own.

## Step 5: Decrypt webhook token (HUMAN)

This produces a secret token. Do not share the output with an LLM.

```bash
sops --decrypt clusters/sauce/flux-system/webhook-token.sops.yaml
# Note the stringData.token value
```

## Step 6: Create webhooks on both repos (HUMAN)

These commands require the webhook secret token from Step 5. Replace `<webhook-path>` and `<webhook-secret>` with actual values.

```bash
for REPO in icook/homelab-gitops icook/tiny-congress; do
  gh api "repos/${REPO}/hooks" \
    --method POST \
    --field name=web \
    --field active=true \
    -f "config[url]=https://flux-webhook.ibcook.com/<webhook-path>" \
    -f "config[content_type]=application/json" \
    -f "config[secret]=<webhook-secret>" \
    --field "events[]=push"
done
```

Verify webhooks were created:

```bash
gh api repos/icook/homelab-gitops/hooks --jq '.[].config.url'
gh api repos/icook/tiny-congress/hooks --jq '.[].config.url'
```

## Step 7: Verify the pipeline (LLM)

Check that the `GITOPS_DEPLOY_KEY` secret exists (does not reveal the value):

```bash
gh secret list --repo icook/tiny-congress | grep GITOPS_DEPLOY_KEY
```

Check that the deploy key exists on the gitops repo:

```bash
gh repo deploy-key list --repo icook/homelab-gitops
```

Watch for a CI run on the latest master push:

```bash
gh run list --repo icook/tiny-congress \
  --branch master --limit 3 \
  --json databaseId,status,conclusion,displayTitle
```

Check `deploy-gitops` job output from the most recent run:

```bash
RUN_ID=$(gh run list --repo icook/tiny-congress \
  --branch master --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$RUN_ID" --repo icook/tiny-congress --log \
  --job "$(gh run view "$RUN_ID" --repo icook/tiny-congress \
    --json jobs --jq '.jobs[] | select(.name == "Update gitops image digests") | .databaseId')"
```

Verify the gitops repo was updated:

```bash
gh api repos/icook/homelab-gitops/commits/main \
  --jq '.commit.message'
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
| Flux not reconciling | Webhook not configured | Check recent deliveries: `gh api repos/icook/homelab-gitops/hooks --jq '.[].last_response'` |
| Pods not updating | Chart doesn't use digest | Verify templates include `@digest` suffix |

## Verification checklist

- [ ] Deploy key with write access exists on `icook/homelab-gitops` (LLM)
- [ ] `GITOPS_DEPLOY_KEY` secret exists on `icook/tiny-congress` (LLM — existence only)
- [ ] Webhooks configured on both repos (LLM — URL check only)
- [ ] `deploy-gitops` job completes on a `master` push (LLM)
- [ ] Gitops repo shows updated digest values after CI run (LLM)
- [ ] Flux reconciles and pods show new image digests (LLM)

## See also
- `.github/workflows/ci.yml` - The `deploy-gitops` job definition
- `kube/app/templates/deployment.yaml` - Helm templates with digest support
- `kube/app/values.yaml` - Default values including `digest: ""`
