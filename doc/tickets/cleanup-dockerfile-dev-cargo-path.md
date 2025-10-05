# Cleanup: Remove Absolute Cargo Path From `service/Dockerfile.dev`

## Summary / Overview
- Document the temporary workaround that hardcodes an absolute Cargo binary path in the dev Dockerfile so we can clean it up later.
- This change was made to unblock local and CI builds but diverges from our usual Dockerfile patterns.

## Goals / Acceptance Criteria
- [ ] Understand why the base Rust image no longer exposes `cargo` on the default `PATH` inside the dev container.
- [ ] Remove the explicit `ENV PATH="/usr/local/cargo/bin:${PATH}"` line (or replace it with a less brittle fix) once the root cause is addressed.
- [ ] Verify `cargo` commands still work for developer workflows and CI builds after the cleanup.
- [ ] Update documentation to reflect the final state of the Dockerfile.

## Additional Context
- The workaround lives in `service/Dockerfile.dev:32` where we prepend `/usr/local/cargo/bin` to `PATH`.
- The absolute path is a stopgap; if the upstream image changes its layout again we might have to revisit.
- Consider whether setting `CARGO_HOME` alone should be sufficient, or if we should rely on the standard `/usr/local/cargo/bin` symlink provided by the Rust image.

## Implementation spec (if known)
- Start with a spike PR that reverts the `PATH` override and confirms builds still succeed.
- If the issue reproduces, document the failure and explore adjusting the base image or leveraging `rustup` toolchains instead of manual PATH surgery.
