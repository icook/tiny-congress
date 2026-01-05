# Agent Contract

Keep this file short and enforceable. Detailed guidance belongs in `docs/`
(start at `docs/README.md`).

## Allowed Actions
- Edit files only in `service/`, `web/`, `kube/`, `dockerfiles/`, `docs/`,
  or repo root configs.
- Use `just --list` for available commands; follow the `justfile`.
- Update docs when behavior or interfaces change.

## Forbidden Actions
- Do not add database tables or migrations without explicit approval.
- Do not modify dependencies without updating the lockfile and calling it out
  in the PR description.
- Do not commit secrets, credentials, or `.env` files.
- Do not delete or rename public API endpoints without a deprecation path.
- Do not modify `skaffold.yaml` profiles without running
  `docs/skills/testing-local-dev.md`.
- Do not run bare `git push`; always use `git push origin <branch-name>`.
- Do not push directly to `master`; use a branch and PR.
- Do not use `--force` or `--force-with-lease` without an explicit branch.

## Required Behavior
- Keep changes scoped to the ticket.
- Do not change crypto semantics without updating explicit test vectors.
- Run the full test command (`just test` by default, or `just test-ci` if the
  issue requests it) and paste the result in the response.
- If tests are skipped, state "Not run (reason)".

## Response Format
- Changes: brief list of edits and where they happened.
- Tests: command(s) run and result, or "Not run (reason)".
- Notes: risks, follow-ups, or "None".
