# Kubernetes Manifests

Helm charts and raw manifests for deploying TinyCongress to Kubernetes.

## Structure

```
kube/
├── app/           # Main application Helm chart
│   ├── Chart.yaml
│   ├── values.yaml
│   └── templates/
└── verify/        # CI verification resources
    ├── test-artifacts-pvc.yaml
    └── test-artifacts-exporter.yaml
```

## app/

Helm chart for the TinyCongress application stack:
- API deployment (`tc-api`)
- Frontend deployment (`tc-frontend`)
- PostgreSQL deployment (dev/CI; use `database.existingSecret` for external Postgres)
- Service and ingress configuration

**Install locally:**
```bash
skaffold dev --port-forward
```

**Configuration:** See `app/values.yaml` for all configurable options.

## verify/

CI-only resources for test artifact collection:
- `test-artifacts-pvc.yaml` - PersistentVolumeClaim for test outputs
- `test-artifacts-exporter.yaml` - Job to export artifacts from cluster

## Skaffold Integration

These manifests are deployed via Skaffold profiles. See:
- [ADR-002: Skaffold orchestration](../docs/decisions/002-skaffold-orchestration.md)
- [skaffold-profiles playbook](../docs/playbooks/skaffold-profiles.md)

## Local Development

Requires a local Kubernetes cluster (KinD recommended):
```bash
kind create cluster
skaffold dev --port-forward
```

See [local-dev-setup playbook](../docs/playbooks/local-dev-setup.md).
