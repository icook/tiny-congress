# testing local dev

Use this skill whenever you need to confirm that the Skaffold-powered developer environment still works end to end.

## Purpose
- Validate that `skaffold dev --port-forward` builds local images without needing registry access.
- Ensure the API and UI are reachable through the forwarded ports.

## Prerequisites
- Docker (or another container runtime) running locally.
- Skaffold installed.
- A local Kubernetes cluster such as Minikube or Colima, with `kubectl config current-context` pointing to it.
- The repository dependencies (Rust toolchain, Node, etc.) installed so the containers can build.

## Procedure
1. Start your local Kubernetes cluster (e.g. `minikube start`) if it is not already running.
2. From the repository root, run:
   ```bash
   skaffold dev --port-forward
   ```
3. Wait for Skaffold to finish building the `tc-api-dev`, `tc-ui-dev`, and `postgres` images and for the deployments to roll out. Successful port-forward setup is indicated by log lines similar to `Port forwarding deployment/tc -> 8080` and `deployment/tc-frontend -> 5173`.
4. In a new terminal, verify the API responds with HTTP 200:
   ```bash
   curl -i http://localhost:8080/health
   ```
5. Visit `http://localhost:5173` in a browser (or run `curl -I http://localhost:5173`) to confirm the frontend is being served.
6. When finished, return to the Skaffold terminal and stop the process with `Ctrl+C`.

## Completion Criteria
- Skaffold runs without attempting to push images to a remote registry.
- The `/health` endpoint returns HTTP 200.
- The frontend responds over port 5173.
- The results above are recorded in the MR discussion when Skaffold configuration changes are proposed.
