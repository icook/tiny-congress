# Bug: Dev/Test Postgres image lacks required `pgmq` extension

## Summary / Overview
- The Helm chart under `kube/app/templates/postgres.yaml` deploys the upstream `postgres:15` image, but our application requires the `pgmq` extension.
- Migrations expect `pgmq` to be present and fail when run against the stock image, breaking local Skaffold dev/test environments.

## Goals / Acceptance Criteria
- [ ] Ensure Kubernetes dev/test deployments use a Postgres image that bundles `pgmq` (e.g., the custom `dockerfiles/Dockerfile.postgres`).
- [ ] Confirm migrations succeed automatically when the pod starts.
- [ ] Document the final image choice so contributors know how to rebuild/push it if needed.
- [ ] Verify both `skaffold dev` and `skaffold test` succeed after the change.

## Additional Context
- The repo already ships `dockerfiles/Dockerfile.postgres` with the extension installed and enables it via init scripts.
- Without `pgmq`, `service/migrations/01_init.sql` fails at the extension creation step; the migration runner exits with an error.
- Developers currently need to manually install the extension inside the pod to continue testing.

## Implementation Spec (if known)
- Update the Helm chart values or templates to reference the custom Postgres image built during Skaffold workflows.
- Regenerate manifests (`skaffold render`) to ensure the new image name is reflected.
- Optionally add a health/readiness check confirming the `pgmq` extension exists before exposing the API deployment.
