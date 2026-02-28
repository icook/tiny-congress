# Updating Dependencies

## When to use
- Adding new crate or npm package
- Updating existing dependencies
- Security vulnerability fixes

## Backend (Rust)

### Adding a dependency
```bash
cd service
cargo add <crate_name>
# or with features:
cargo add <crate_name> --features feature1,feature2
```

### Updating dependencies
```bash
cd service
cargo update                    # Update all within semver
cargo update -p <crate_name>    # Update specific crate
```

### After any change
```bash
just lint-backend      # Format + clippy
just test-backend      # Run tests
```

## Frontend (Node/Yarn)

### Adding a dependency
```bash
cd web
yarn add <package>              # Runtime dependency
yarn add -D <package>           # Dev dependency
```

### Updating dependencies
```bash
cd web
yarn up <package>               # Update specific package
yarn up '*'                     # Update all packages
```

### After any change
```bash
just lint-frontend     # Prettier + ESLint + Stylelint
just typecheck         # TypeScript checking
just test-frontend     # Run tests
```

## CI cache invalidation

Lockfile changes automatically invalidate caches:
- `Cargo.lock` → Rust build cache
- `yarn.lock` → Node modules cache, Playwright browsers

No manual action needed.

## Verification
- [ ] Lockfile committed (`Cargo.lock` or `yarn.lock`)
- [ ] `cargo check` / `yarn typecheck` passes
- [ ] Tests pass locally
- [ ] No new security advisories: `cargo audit` / `yarn audit`

## Common failures

| Error | Cause | Fix |
|-------|-------|-----|
| "lockfile out of date" | Edited Cargo.toml without cargo | Run `cargo check` |
| yarn immutable failure | yarn.lock mismatch | Run `yarn install` locally |
| Version conflict | Incompatible peer deps | Check compatibility matrix |

## Prohibited actions
- DO NOT add dependencies without updating lockfile
- DO NOT commit node_modules or target/
- DO NOT use `--force` or `--ignore-scripts` without security review

## See also
- `service/Cargo.toml` - Rust dependencies
- `web/package.json` - Node dependencies
