# ADR-032: CI Workload Isolation — Ephemeral Namespaces on Shared Cluster

## Status
Accepted

Co-tenancy of CI and production workloads on the shared cluster is acceptable through private beta (~100 users). Hard triggers for cluster separation are enumerated below; any one firing requires revisiting this decision. See GitHub #697 for implementation plan.

## Context

ARC (Actions Runner Controller) runs ephemeral GitHub Actions runner pods on the Talos K8s 1.34 cluster. The current runner configuration includes a Docker-in-Docker sidecar that burns 300–580m CPU at idle per pod — the single largest resource inefficiency in CI. Eliminating dind requires an alternative way for CI jobs to access infrastructure (Postgres for tests, app deployments for E2E).

Three jobs use Docker today:
- `build-images` — already migrated to remote BuildKit, dind not needed
- `rust-checks` — `docker run` for Postgres, replaceable
- `e2e-tests` — KinD cluster via Docker, replaceable

The fundamental question: can CI workloads safely create and manage test infrastructure on the same cluster that runs production?

## Decision

### CI jobs orchestrate their own infrastructure in ephemeral Kubernetes namespaces.

Each CI job that needs infrastructure creates a `ci-{run_id}` namespace, deploys resources (Postgres, app services) into it, runs tests, and deletes the namespace. A single lean runner pool (no dind sidecar) serves all 17 CI jobs.

### Kyverno manages the security-sensitive resources, not the CI ServiceAccount.

The CI ServiceAccount (`ci-runner` in `arc-runners`) gets exactly one cluster-level permission: create and delete namespaces. Kyverno policies handle everything else:

- **Validate** namespace names to enforce `ci-*` prefix
- **Generate** RoleBinding, NetworkPolicy, ResourceQuota, LimitRange, and PSS labels when a `ci-*` namespace is created
- **Validate** image pull sources (GHCR + in-cluster Zot only)
- **Validate** `automountServiceAccountToken: false` for pods in `ci-*` namespaces
- **Cleanup** namespaces older than 2h via ClusterCleanupPolicy

This design was chosen over SA-managed RoleBindings because granting `create rolebindings` at cluster scope allows a compromised job to bind itself into any namespace — including production. Kyverno generation closes this escalation path entirely.

### Shared cluster is acceptable at current scale.

Namespaces are an organizational boundary, not a security boundary. A container escape on a shared node reaches all pods on that node. The hardening stack (PSS restricted, Cilium NetworkPolicy, ResourceQuota) addresses API, network, and resource exhaustion vectors but not shared-kernel risk.

This is acceptable because:
- Pre-launch demo and private beta users expect instability
- No PII beyond public keys is stored
- No money or legal consequences flow through the system
- Blast radius of a CI compromise is "reset the demo and re-key users"
- The design is cluster-topology-agnostic — splitting later is a deployment change, not an architecture change

## Hard triggers for cluster separation

Any one of these conditions firing means CI must move to a dedicated cluster or at minimum a dedicated node pool with taints:

1. Real money or legal consequences flow through the system
2. PII beyond public keys is stored (email, phone, legal name)
3. Regulatory scope applies (SOC 2, GDPR, election law)
4. Multiple external contributors can open PRs that trigger CI
5. An uptime SLA exists (CI resource exhaustion can evict production pods)
6. Re-keying all users after an incident is unacceptable

**Intermediate step before full separation:** label dedicated CI nodes with `node-role.tinycongress.io/ci` and use node affinity (soft initially, hard + taints when a trigger fires). This isolates container escape blast radius without a second cluster.

## Security layers

| Layer | Addresses | Status |
|-------|-----------|--------|
| Kyverno validate: `ci-*` prefix | Namespace squatting, rogue creation | Planned (Phase 0) |
| Kyverno generate: RoleBinding | RBAC escalation via rolebinding injection | Planned (Phase 0) |
| Kyverno generate: NetworkPolicy | Lateral movement to production | Planned (Phase 0) — Cilium already deployed |
| Kyverno generate: ResourceQuota + LimitRange | Resource exhaustion DoS | Planned (Phase 0) |
| Kyverno mutate: PSS restricted | Privileged container / node escape | Planned (Phase 0) |
| Kyverno validate: image registries | Supply chain via untrusted registries | Planned (Phase 0) |
| Kyverno validate: automount=false | Token exposure in test pods | Planned (Phase 0) |
| Kyverno ClusterCleanupPolicy + CronJob | Orphaned namespace accumulation | Planned (Phase 0) |
| Node affinity for CI pods | Container escape blast radius | Planned (Phase 4 / before public beta) |
| Dedicated cluster | Full isolation | When a hard trigger fires |

## Alternatives considered

**SA-managed RoleBindings (SA creates its own RoleBinding in ci-\* namespaces):** Rejected. Requires granting `create rolebindings` at cluster scope, which permits binding into any namespace. The escalation path from CI compromise to production secrets access is a single kubectl command.

**Dedicated CI cluster from day one:** Rejected. Doubles infrastructure cost and operational overhead for a solo-dev pre-launch project. The threat model doesn't justify it — blast radius is acceptable at current scale. The design is structured so migration to a dedicated cluster is a deployment-only change.

**vCluster (virtual clusters per CI run):** Rejected. 30–60s startup overhead per run. Better isolation than namespaces but overkill unless CI jobs need cluster-admin privileges. Revisit if test workloads require CRD installation or cluster-scoped resources.

**Postgres sidecar in runner pod (no namespace creation):** Kept as fallback. Simplest possible design — zero RBAC, zero Kyverno, zero namespace management. Loses per-run database isolation for concurrent jobs. Viable if CI jobs are serialized; breaks down under parallel execution.

**gVisor/Kata Containers (runtime sandboxing):** Rejected. Addresses the shared-kernel risk that namespaces don't, but at 2–5x I/O overhead for syscall-heavy workloads (Postgres, Rust compilation). The performance cost exceeds the security benefit at this scale.

**Specialized runner pools (dind pool + lean pool):** Rejected. Fragments the warm pool, reduces reusability, and maintains dind operational complexity for a shrinking number of jobs.

## Consequences

- All CI jobs run on a single lean runner pool — no dind sidecar, lower idle resource burn
- CI jobs that need infrastructure create ephemeral namespaces via kubectl — ~5–15s startup vs ~60–90s for KinD
- Kyverno becomes a CI infrastructure dependency — if Kyverno is unavailable, CI namespace creation still succeeds but without the generated security resources (NetworkPolicy, ResourceQuota, RoleBinding). Kyverno's `failurePolicy: Fail` on validation policies prevents namespace creation without policy enforcement.
- The cluster separation decision must be actively revisited when approaching public beta or when any hard trigger fires
- Runner image needs no changes (kubectl already baked in); RBAC and Kyverno policies are deployed via homelab-gitops
