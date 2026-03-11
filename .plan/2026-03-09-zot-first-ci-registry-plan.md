# Zot-First CI Registry Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Route ARC CI image pushes/pulls through in-cluster zot, promoting to GHCR only on master merge. GHA path unchanged.

**Architecture:** Add `image-registry` output to `detect-changes`. All jobs use it for image refs. On ARC, images live in zot during CI; `deploy-gitops` promotes to GHCR via `crane copy`. On GHA, everything goes to GHCR as today.

**Tech Stack:** GitHub Actions YAML, shell, crane CLI, KinD containerd config

**Design doc:** `.plan/2026-03-09-zot-first-ci-registry-design.md`

---

### Task 1: Add `image-registry` output to `detect-changes`

**Files:**
- Modify: `.github/workflows/ci.yml:416-458`

**Step 1: Add the output declaration**

In the `detect-changes` job `outputs:` block (line 416-430), add after `use-arc`:

```yaml
      # Image registry — where built images are pushed/pulled during CI.
      image-registry: ${{ steps.runner.outputs.image-registry }}
```

**Step 2: Set the output in the `Select runner` step**

In the ARC branch of the if/else (line 447-451), add:

```bash
echo "image-registry=zot.zot.svc.cluster.local:5000/images" >> "$GITHUB_OUTPUT"
```

In the GHA branch (line 453-457), add:

```bash
echo "image-registry=ghcr.io/icook/tiny-congress" >> "$GITHUB_OUTPUT"
```

**Step 3: Add to `Print runner mode` step**

After the existing notice line, add:

```bash
echo "::notice title=Image Registry::${{ steps.runner.outputs.image-registry }}"
```

**Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add image-registry output to detect-changes

ARC: zot.zot.svc.cluster.local:5000/images
GHA: ghcr.io/icook/tiny-congress"
```

---

### Task 2: Route `build-images` to use `image-registry`

**Files:**
- Modify: `.github/workflows/ci.yml:492-631`

**Step 1: Replace `Configure cache registry` with `Configure registries`**

Replace the existing `Configure cache registry` step (line 536-543) with:

```yaml
      - name: Configure registries
        id: registries
        run: |
          IMAGE_REGISTRY="${{ needs.detect-changes.outputs.image-registry }}"
          if [ "$RUNNER_ENVIRONMENT" = "self-hosted" ]; then
            echo "cache=zot.zot.svc.cluster.local:5000/cache" >> "$GITHUB_OUTPUT"
          else
            echo "cache=${IMAGE_REGISTRY}" >> "$GITHUB_OUTPUT"
          fi
          echo "images=${IMAGE_REGISTRY}" >> "$GITHUB_OUTPUT"
```

**Step 2: Make GHCR login conditional**

Change the `Login to GHCR` step (line 545-550) to skip on ARC:

```yaml
      - name: Login to GHCR
        if: runner.environment != 'self-hosted'
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
```

**Step 3: Update tag computation**

In the `Compute tags` step (line 552-562), replace `${{ env.REGISTRY }}` with `${{ steps.registries.outputs.images }}`:

```yaml
      - name: Compute tags
        id: tags
        run: |
          IMAGES="${{ steps.registries.outputs.images }}"
          ref="${GITHUB_HEAD_REF:-$GITHUB_REF_NAME}"
          branch="${ref//[^a-zA-Z0-9_.-]/-}"
          branch="${branch,,}"
          {
            echo "branch_slug=${branch}"
            echo "sha_tag=${IMAGES}/${{ matrix.image }}:${{ github.sha }}"
            echo "branch_tag=${IMAGES}/${{ matrix.image }}:branch-${branch}"
          } >> "$GITHUB_OUTPUT"
```

**Step 4: Update retag step**

In `Retag existing image` (line 580-588), replace `${{ env.REGISTRY }}` with `${{ steps.registries.outputs.images }}`:

```yaml
      - name: Retag existing image
        if: steps.should-build.outputs.changed == 'false'
        run: |
          SOURCE="${{ steps.registries.outputs.images }}/${{ matrix.image }}:branch-master"
          echo "No source changes for ${{ matrix.image }} — retagging ${SOURCE} as ${{ github.sha }}"
          docker buildx imagetools create \
            --tag "${{ steps.tags.outputs.sha_tag }}" \
            "${SOURCE}"
```

**Step 5: Update cache-from/cache-to refs**

In `Build and push` (line 604-625), replace `${{ steps.cache.outputs.registry }}` with `${{ steps.registries.outputs.cache }}`:

```yaml
          cache-from: |
            type=registry,ref=${{ steps.registries.outputs.cache }}/${{ matrix.image }}:cache
          cache-to: |
            type=registry,ref=${{ steps.registries.outputs.cache }}/${{ matrix.image }}:cache,mode=max
```

**Step 6: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: route build-images push/pull through image-registry

On ARC, images push to zot (in-cluster). On GHA, push to GHCR.
GHCR login skipped on ARC since it's not needed."
```

---

### Task 3: Route `scan-images` to use `image-registry`

**Files:**
- Modify: `.github/workflows/ci.yml:632-681`

**Step 1: Make GHCR login conditional**

Change the `Login to GHCR` step (line 663-669) to skip on ARC:

```yaml
      - name: Login to GHCR
        if: steps.should-scan.outputs.changed != 'false' && runner.environment != 'self-hosted'
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
```

**Step 2: Update Trivy image-ref**

In `Run Trivy` (line 671-681), replace `${{ env.REGISTRY }}` with the image-registry output. Also add env for insecure registry on ARC:

```yaml
      - name: Run Trivy vulnerability scanner
        if: steps.should-scan.outputs.changed != 'false'
        uses: aquasecurity/trivy-action@0.34.0
        env:
          TRIVY_INSECURE: ${{ runner.environment == 'self-hosted' && 'true' || 'false' }}
        with:
          image-ref: ${{ needs.detect-changes.outputs.image-registry }}/${{ matrix.image }}:${{ github.sha }}
          format: 'table'
          exit-code: '1'
          ignore-unfixed: true
          severity: 'CRITICAL,HIGH'
          trivyignores: '.trivyignore'
          version: 'v0.65.0'
```

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: route scan-images through image-registry

Trivy pulls from zot on ARC (with TRIVY_INSECURE for HTTP).
GHCR login skipped on ARC."
```

---

### Task 4: Route `sqlx-check` to use `image-registry`

**Files:**
- Modify: `.github/workflows/ci.yml:320-382`

**Step 1: Make GHCR login conditional**

Change the `Login to GHCR` step (line 332-337) to skip on ARC:

```yaml
      - name: Login to GHCR
        if: runner.environment != 'self-hosted'
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
```

**Step 2: Update postgres image ref**

In `Start postgres` (line 339-381), replace `${{ env.REGISTRY }}` on line 356 with the image-registry output:

```yaml
              ${{ needs.detect-changes.outputs.image-registry }}/postgres:${{ github.sha }}
```

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: route sqlx-check postgres pull through image-registry"
```

---

### Task 5: Route `rust-coverage` to use `image-registry`

**Files:**
- Modify: `.github/workflows/ci.yml:726-743`

**Step 1: Update TEST_POSTGRES_IMAGE env**

Replace the env block (line 732-733):

```yaml
    env:
      TEST_POSTGRES_IMAGE: ${{ needs.detect-changes.outputs.image-registry }}/postgres:${{ github.sha }}
```

Note: remove the `|| format(...)` fallback — the image-registry output is always set.

**Step 2: Make GHCR login conditional**

Change the `Login to GHCR` step (line 738-743) to skip on ARC:

```yaml
      - name: Login to GHCR
        if: runner.environment != 'self-hosted'
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
```

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: route rust-coverage postgres pull through image-registry"
```

---

### Task 6: Route `e2e-tests` to use `image-registry`

**Files:**
- Modify: `.github/workflows/ci.yml:768-927`

**Step 1: Update SKAFFOLD_DEFAULT_REPO env**

Replace the hardcoded env (line 791):

```yaml
      SKAFFOLD_DEFAULT_REPO: ${{ needs.detect-changes.outputs.image-registry }}
```

**Step 2: Update Skaffold artifacts file**

Replace the `Write Skaffold artifacts file` step (line 837-847). Use the image-registry output:

```yaml
      - name: Write Skaffold artifacts file
        id: artifacts
        run: |
          set -euo pipefail
          IMAGE_REG="${{ needs.detect-changes.outputs.image-registry }}"
          artifacts_file="${RUNNER_TEMP}/skaffold-artifacts.json"
          printf '{\n  "builds": [\n    {"imageName": "tc-api-release", "tag": "%s/tc-api-release:%s"},\n    {"imageName": "tc-ui-release", "tag": "%s/tc-ui-release:%s"},\n    {"imageName": "postgres", "tag": "%s/postgres:%s"}\n  ]\n}\n' \
            "$IMAGE_REG" "${{ github.sha }}" \
            "$IMAGE_REG" "${{ github.sha }}" \
            "$IMAGE_REG" "${{ github.sha }}" \
            > "$artifacts_file"
          echo "file=${artifacts_file}" >> "$GITHUB_OUTPUT"
```

**Step 3: Add KinD containerd mirror config for ARC**

In `Start KinD cluster in background` (line 813-821), add containerd config for zot on ARC. Replace the `kind create cluster` line:

```yaml
      - name: Start KinD cluster in background
        run: |
          set -euo pipefail
          KIND_LOG="${RUNNER_TEMP}/kind.log"
          echo "KIND_LOG=${KIND_LOG}" >> "$GITHUB_ENV"

          # On ARC, configure KinD's containerd to use zot as a mirror
          # so pods can pull CI-built images without egress.
          KIND_CONFIG="${RUNNER_TEMP}/kind-config.yaml"
          cat > "$KIND_CONFIG" <<KINDEOF
          kind: Cluster
          apiVersion: kind.x-k8s.io/v1alpha4
          KINDEOF

          if [ "$RUNNER_ENVIRONMENT" = "self-hosted" ]; then
            cat >> "$KIND_CONFIG" <<KINDEOF
          containerdConfigPatches:
            - |-
              [plugins."io.containerd.grpc.v1.cri".registry.mirrors."zot.zot.svc.cluster.local:5000"]
                endpoint = ["http://zot.zot.svc.cluster.local:5000"]
          KINDEOF
          fi

          echo "Starting KinD cluster ${KIND_CLUSTER_NAME} (logs: ${KIND_LOG})"
          kind create cluster --name "${KIND_CLUSTER_NAME}" --config "${KIND_CONFIG}" --wait 0s >"${KIND_LOG}" 2>&1 &
```

**Step 4: Make GHCR login conditional**

Change the `Login to GHCR` step (line 830-835) to skip on ARC:

```yaml
      - name: Login to GHCR
        if: runner.environment != 'self-hosted'
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
```

**Step 5: Make `Configure registry access for KinD` conditional**

The imagePullSecrets for GHCR aren't needed on ARC (zot is unauthenticated). Add `if`:

```yaml
      - name: Configure registry access for KinD
        if: runner.environment != 'self-hosted'
        run: |
          ...existing code...
```

**Step 6: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: route e2e-tests through image-registry

Skaffold artifacts and default repo use zot on ARC.
KinD containerd configured with zot mirror for pod image pulls.
GHCR login and imagePullSecrets skipped on ARC."
```

---

### Task 7: Add promote step to `deploy-gitops`

**Files:**
- Modify: `.github/workflows/ci.yml:1063-1097`

**Step 1: Add `packages: write` permission**

The deploy-gitops job needs write access to push to GHCR. Update permissions (line 1073-1075):

```yaml
    permissions:
      contents: read
      packages: write
```

**Step 2: Add crane install + promote step**

Insert after `Checkout gitops repo` (line 1077-1082) and before `Get image digests from GHCR` (line 1084):

```yaml
      - name: Install crane
        if: needs.detect-changes.outputs.use-arc == 'true'
        run: |
          CRANE_VERSION=0.20.3
          curl -fsSL "https://github.com/google/go-containerregistry/releases/download/v${CRANE_VERSION}/go-containerregistry_Linux_x86_64.tar.gz" \
            | sudo tar -xz -C /usr/local/bin crane

      - name: Login to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Promote images from zot to GHCR
        if: needs.detect-changes.outputs.use-arc == 'true'
        run: |
          set -euo pipefail
          ZOT="${{ needs.detect-changes.outputs.image-registry }}"
          GHCR="${{ env.REGISTRY }}"
          SHA="${{ github.sha }}"
          ref="${GITHUB_HEAD_REF:-$GITHUB_REF_NAME}"
          branch="${ref//[^a-zA-Z0-9_.-]/-}"
          branch="${branch,,}"
          for IMAGE in tc-api-release tc-ui-release postgres; do
            echo "Promoting ${IMAGE}..."
            crane copy --insecure "${ZOT}/${IMAGE}:${SHA}" "${GHCR}/${IMAGE}:${SHA}"
            crane copy --insecure "${ZOT}/${IMAGE}:branch-${branch}" "${GHCR}/${IMAGE}:branch-${branch}"
          done
```

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add promote step to copy images from zot to GHCR

On ARC, images live in zot during CI. After all tests pass on master,
crane copies them to GHCR for deployment. On GHA, images are already
in GHCR so the promote step is skipped."
```

---

### Task 8: Add network diagnostics shim

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add a reusable diagnostic step to `build-images` and `e2e-tests`**

Insert right after `Checkout` in both jobs:

```yaml
      - name: Network diagnostics
        if: runner.environment == 'self-hosted'
        continue-on-error: true
        run: |
          set -x
          for host in auth.docker.io registry-1.docker.io ghcr.io api.github.com zot.zot.svc.cluster.local; do
            echo "--- $host ---"
            nslookup "$host" 2>&1 | head -6 || true
          done
          for url in https://ghcr.io/v2/ http://zot.zot.svc.cluster.local:5000/v2/; do
            curl -sS -o /dev/null -w "dns:%{time_namelookup} tcp:%{time_connect} tls:%{time_appconnect} total:%{time_total} http:%{http_code}\n" \
              --max-time 10 "$url" || true
          done
```

**Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add network diagnostics shim for ARC runners

Non-blocking step that logs DNS resolution and connectivity timing
to key registries. Helps diagnose egress issues in CI logs."
```

---

### Task 9: Validate and push

**Step 1: Lint the workflow YAML**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML valid"
```

**Step 2: Review the full diff**

```bash
git diff master -- .github/workflows/ci.yml
```

Verify:
- All `${{ env.REGISTRY }}` references in image paths are replaced with `image-registry` or `registries.outputs.images`
- `${{ env.REGISTRY }}` in the global `env:` block (line 17) is preserved — it's still used by `deploy-gitops` for the GHCR target
- GHCR login steps have `if: runner.environment != 'self-hosted'` (except deploy-gitops which always needs it)
- No leftover `${{ steps.cache.outputs.registry }}` — replaced with `${{ steps.registries.outputs.cache }}`

**Step 3: Push and open PR**

```bash
git push origin feature/zot-first-ci-registry
```

Open a draft PR linking to the design doc. Title: `ci: zot-first image registry for ARC runners`

Note in PR description: **requires homelab-gitops infra changes first** (zot sync config, buildkitd mirrors, DinD daemon.json). Without those, ARC builds will fail to push to zot. Land infra first, then merge this.
