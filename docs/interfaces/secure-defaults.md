# Secure Defaults Policy

This document establishes the security configuration policy for TinyCongress. All configuration defaults must follow these principles.

## Core Principles

### 1. Deny by Default

Security-sensitive features default to the most restrictive option, not the most permissive.

```rust
// BAD: Defaults to permissive
fn default_allowed_origins() -> Vec<String> {
    vec!["*".into()]  // Allows any origin - security risk
}

// GOOD: Defaults to restrictive
fn default_allowed_origins() -> Vec<String> {
    vec![]  // Blocks all cross-origin requests until explicitly configured
}
```

### 2. Explicit Opt-In

Permissive or dangerous settings require explicit configuration. Users must consciously choose to reduce security.

```yaml
# Production-safe by default - no action needed
cors:
  allowed_origins: []  # Default: blocks all

# Explicit opt-in required for permissive behavior
cors:
  allowed_origins:
    - https://app.example.com
```

### 3. Loud Failures

Misconfiguration should fail visibly at startup, not silently degrade security at runtime.

```rust
// BAD: Silent fallback
let origins = config.cors.allowed_origins.unwrap_or(vec!["*".into()]);

// GOOD: Fail-fast with clear error
if config.database.url.is_empty() {
    return Err(ConfigError::Validation(
        "database.url is required. Set TC_DATABASE__URL environment variable.".into(),
    ));
}
```

### 4. Dev vs Prod Separation

Development conveniences belong in example config files, not compiled defaults.

| File | Purpose | Checked In |
|------|---------|------------|
| `config.yaml.example` | Development-friendly defaults | Yes |
| `config.yaml` | Local dev config | No (gitignored) |
| Environment variables | Production overrides | N/A |
| Kubernetes secrets | Sensitive production values | No |

## Configuration Categories

| Category | Default Behavior | Rationale |
|----------|------------------|-----------|
| CORS origins | Empty (block all) | Prevents CSRF, restricts API access |
| Debug logging | `info` level | Avoids leaking sensitive data |
| Database URL | Required (no default) | Forces explicit connection config |
| Server port | `8080` | Non-privileged port, standard for containers |
| TLS verification | Enabled | Prevents MITM attacks |
| Rate limiting | Enabled with conservative limits | Prevents DoS (when implemented) |
| Authentication | Required | Prevents unauthorized access (when implemented) |

## Implementation Checklist

When adding new configuration options:

- [ ] Default to the most restrictive secure value
- [ ] Require explicit opt-in for permissive settings
- [ ] Add validation that fails fast on invalid values
- [ ] Document the default and security implications
- [ ] Add development-friendly values to `config.yaml.example`
- [ ] Update `docs/interfaces/environment-variables.md`

## Current Implementations

### CORS (Implemented)

**Default:** Empty list (blocks all cross-origin requests)

```rust
fn default_allowed_origins() -> Vec<String> {
    vec![]  // Safe default - configure explicitly
}
```

**Warning on permissive config:**
```rust
if cors_origins.iter().any(|o| o == "*") {
    tracing::warn!("CORS configured to allow any origin - not recommended for production");
}
```

See: `service/src/config.rs`, PR #97

### Database Connection (Implemented)

**Default:** User and password are required (no compiled-in defaults).

Validation fails immediately if `database.user` or `database.password` is empty.

```rust
if self.database.user.is_empty() {
    return Err(ConfigError::Validation(
        "database.user is required. Set TC_DATABASE__USER environment variable or configure in config.yaml.".into(),
    ));
}
```

Non-secret fields have sensible defaults: `host: localhost`, `port: 5432`, `name: tiny-congress`.

See: `service/src/config.rs`

### Logging Level (Implemented)

**Default:** `info`

Production-safe default that avoids verbose debug output which might leak sensitive information.

## Anti-Patterns to Avoid

### 1. Permissive Defaults

```rust
// AVOID: Makes insecure the path of least resistance
fn default_rate_limit() -> u32 {
    0  // Disabled by default
}
```

### 2. Silent Degradation

```rust
// AVOID: Hides misconfiguration
let pool_size = config.max_connections.unwrap_or(100);  // Silent fallback
```

### 3. Development Defaults in Code

```rust
// AVOID: Development convenience compiled into production
fn default_log_level() -> String {
    "debug".to_string()  // Too verbose for production
}
```

### 4. Optional Security Features

```rust
// AVOID: Security should not be optional
#[serde(default)]
pub enable_auth: bool,  // Defaults to false
```

## References

- [OWASP Secure Configuration Guidelines](https://owasp.org/www-project-web-security-testing-guide/latest/4-Web_Application_Security_Testing/02-Configuration_and_Deployment_Management_Testing/)
- PR #97 - CORS origin restriction (established this pattern)
- `docs/interfaces/environment-variables.md` - Configuration reference

## See Also

- `service/src/config.rs` - Configuration implementation
- `service/config.yaml.example` - Development configuration template
- `CLAUDE.md` - Repository guidelines
