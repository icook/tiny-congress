# Bug: `skaffold test` fails locally due to missing image tag env vars

## Summary / Overview
- Running `skaffold test` on a developer machine fails during the custom test stage because the test command references `${SKAFFOLD_DEFAULT_REPO}` and `${GITHUB_SHA}`.
- Those environment variables are not set outside of CI, so the resulting image reference is malformed (e.g., `//tc-api-dev:`) and Docker rejects the command before tests start.

## Goals / Acceptance Criteria
- [ ] Update `skaffold.yaml` so local test runs use valid image references without relying on CI-only environment variables.
- [ ] Ensure CI continues to use the correct fully-qualified image names.
- [ ] Verify `skaffold test` succeeds locally after the change.
- [ ] Capture the resolution in docs or release notes if developer workflow changes.

## Additional Context
- The failure happens in `skaffold.yaml` within the `test` section that launches Docker containers for API and UI tests.
- Skaffold exposes a per-artifact `IMAGE=<name:tag>` environment variable to custom test commands; relying on that keeps the configuration portable between local and CI runs.
- CI currently injects `${SKAFFOLD_DEFAULT_REPO}` and `${GITHUB_SHA}`, so this bug only affects local runs.

## Implementation Spec (if known)
- Replace the raw `${SKAFFOLD_DEFAULT_REPO}/tc-api-dev:${GITHUB_SHA}` strings with the `$IMAGE` environment variable Skaffold injects for each custom tester.
- Run `skaffold test -p dev` locally to confirm both API and UI test commands execute.
