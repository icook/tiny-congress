# Verify artifacts via PVC

Integration tests now run as a Kubernetes Job that writes coverage to a shared PVC. The job manifest lives at `kube/verify/integration-tests-job.yaml` and mounts `test-artifacts-pvc` at `/artifacts` (where `backend-integration.lcov` is written).

## Local flow

```bash
# Ensure the PVC exists (once per cluster)
kubectl apply -f kube/verify/test-artifacts-pvc.yaml

# Run verify (uses the Job manifest)
skaffold verify -p ci

# Launch the exporter and wait for it to be Ready
kubectl apply -f kube/verify/test-artifacts-exporter.yaml
kubectl wait --for=condition=Ready pod/test-artifacts-exporter --timeout=60s

# Copy artifacts locally
mkdir -p service/coverage
kubectl cp test-artifacts-exporter:/artifacts/. service/coverage

# Clean up the helper pod
kubectl delete pod test-artifacts-exporter
```

CI follows the same pattern: apply the PVC, run `skaffold verify`, start the exporter, `kubectl cp` the artifacts, and delete the helper pod.
