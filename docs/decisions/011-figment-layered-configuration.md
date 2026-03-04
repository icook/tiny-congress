# ADR-011: Figment Layered Configuration

## Status
Accepted

## Context

TinyCongress runs in multiple environments — local development, KinD-based CI, and Kubernetes production. Each environment has different configuration needs: local dev wants defaults that work out of the box, CI needs to override database hosts, and production needs secrets injected from external secret managers. The configuration system must handle all three without environment-specific code paths.

Several tensions shaped this decision:

- **Security vs. convenience.** Embedding a database URL with credentials in a ConfigMap is convenient but leaks secrets to anyone who can read Kubernetes resources. Splitting credentials into Secrets adds complexity but follows the principle of least privilege.
- **Fail-fast vs. permissive.** A permissive config system that silently defaults missing values is easier to start with but produces confusing runtime failures (e.g., empty CORS allows nothing, but doesn't explain why). Fail-fast validation catches misconfigurations at startup.
- **Single image vs. per-environment builds.** Baking configuration into Docker images is simpler but requires a rebuild per environment. Runtime configuration injection lets one image serve all environments.

## Decision

### Four-layer Figment stack

Configuration loads through four layers, each overriding the previous:

```rust
pub fn load() -> Result<Self, ConfigError> {
    let config: Self = Figment::new()
        .merge(Serialized::defaults(Self::default()))  // Layer 1: Rust struct defaults
        .merge(Yaml::file("/etc/tc/config.yaml"))      // Layer 2: Kubernetes ConfigMap mount
        .merge(Yaml::file("config.yaml"))              // Layer 3: Local dev override
        .merge(Env::prefixed("TC_").split("__"))       // Layer 4: Environment variables
        .extract()?;

    config.validate()?;
    Ok(config)
}
```

| Layer | Source | Purpose |
|-------|--------|---------|
| 1 (lowest) | Rust struct defaults | Sensible defaults for all fields |
| 2 | `/etc/tc/config.yaml` | Kubernetes ConfigMap mounted as a volume |
| 3 | `config.yaml` (cwd) | Local development overrides (gitignored) |
| 4 (highest) | `TC_*` environment variables | Per-container overrides, secrets |

Environment variables use `TC_` prefix with `__` (double underscore) for nesting: `TC_DATABASE__USER`, `TC_CORS__ALLOWED_ORIGINS`.

### Database credentials as individual fields

Database configuration uses individual typed fields instead of a connection URL:

```rust
pub struct DatabaseConfig {
    pub host: String,           // default: "localhost"
    pub port: u16,              // default: 5432
    pub name: String,           // default: "tiny-congress"
    pub user: String,           // no default (required)
    pub password: String,       // no default (required)
    pub max_connections: u32,   // default: 10
    pub migrations_dir: Option<String>,
}
```

The `connect_options()` method builds `PgConnectOptions` from individual fields:

```rust
pub fn connect_options(&self) -> PgConnectOptions {
    PgConnectOptions::new()
        .host(&self.host)
        .port(self.port)
        .database(&self.name)
        .username(&self.user)
        .password(&self.password)
}
```

This avoids URL-encoding issues with special characters in passwords (`@`, `:`, `/`, `?`), makes each field independently overridable via environment variables, and keeps credentials in dedicated fields rather than embedded in a URL string.

### ConfigMap/Secret split

Non-sensitive configuration lives in a Kubernetes ConfigMap, mounted at `/etc/tc/config.yaml`:

```yaml
# ConfigMap contents
database:
  host: {{ include "app.databaseHost" . }}
  port: {{ .Values.database.port }}
  name: {{ .Values.database.name }}
  max_connections: {{ .Values.database.maxConnections }}
server:
  port: {{ .Values.service.port }}
cors:
  allowed_origins: {{ .Values.cors.allowedOrigins }}
graphql:
  playground_enabled: {{ .Values.graphql.playgroundEnabled }}
swagger:
  enabled: {{ .Values.swagger.enabled }}
```

Sensitive credentials live in a Kubernetes Secret, injected as environment variables in the Deployment:

```yaml
env:
  - name: TC_DATABASE__USER
    valueFrom:
      secretKeyRef:
        name: {{ .Values.database.existingSecret | default (printf "%s-database" (include "app.fullname" .)) }}
        key: user
  - name: TC_DATABASE__PASSWORD
    valueFrom:
      secretKeyRef:
        name: {{ .Values.database.existingSecret | default ... }}
        key: password
```

This separation ensures that RBAC policies can grant ConfigMap read access broadly while restricting Secret access.

### `existingSecret` pattern for external secret managers

The Helm chart conditionally creates its own Secret:

```yaml
{{- if not .Values.database.existingSecret }}
apiVersion: v1
kind: Secret
metadata:
  name: {{ include "app.fullname" . }}-database
stringData:
  user: {{ .Values.database.user }}
  password: {{ .Values.database.password }}
{{- end }}
```

When `database.existingSecret` is set (e.g., to a Secret created by External Secrets Operator, Sealed Secrets, or Vault), the chart skips Secret creation and references the external Secret directly. The Deployment's `secretKeyRef` uses a Helm expression that resolves to either the chart-managed or external Secret name.

### Secure defaults

All defaults follow the principle of "safe in production, explicitly override for development":

- **CORS origins:** Empty list (no cross-origin requests). Must be explicitly configured.
- **GraphQL Playground:** Disabled (`playground_enabled: false`). Enable via `TC_GRAPHQL__PLAYGROUND_ENABLED=true`.
- **Swagger UI:** Disabled (`enabled: false`). Enable via `TC_SWAGGER__ENABLED=true`.
- **Security headers:** Enabled by default. `X-Frame-Options: DENY`, `Content-Security-Policy: default-src 'self'`, `Referrer-Policy: strict-origin-when-cross-origin`. HSTS disabled by default (requires HTTPS).
- **Database credentials:** No compiled-in defaults for `user` and `password`. Startup fails if not provided.

Runtime logging announces the state of optional features:
```
GraphQL Playground disabled (enable via TC_GRAPHQL__PLAYGROUND_ENABLED=true)
Swagger UI disabled (enable via TC_SWAGGER__ENABLED=true)
```

### Fail-fast validation at startup

`Config::load()` calls `validate()` immediately after extraction. The application exits before binding any port or opening any connection if validation fails:

```rust
let config = Config::load().map_err(|e| anyhow::anyhow!("{e}"))?;
```

Validation rules:
- `database.user` and `database.password` must be non-empty
- `database.port` and `server.port` must be non-zero
- `database.max_connections` must be >= 1
- CORS origins must be `"*"` or start with `http://` / `https://`
- `security_headers.frame_options` must be `"DENY"` or `"SAMEORIGIN"` (case-insensitive)

Error messages include remediation hints (e.g., `"Set TC_DATABASE__USER environment variable or configure in config.yaml"`).

## Consequences

### Positive
- One config system works identically across local dev, CI, and production — no environment-detection code.
- Credentials never appear in ConfigMaps or Helm values files checked into version control.
- Fail-fast validation catches misconfigurations at startup, not minutes later when the first request exercises a missing value.
- The `existingSecret` pattern integrates with any Kubernetes secret management solution without chart modifications.
- Individual database fields eliminate URL-encoding bugs and make each field independently overridable.

### Negative
- Four layers of override precedence can make it hard to debug which layer provided a given value. No "show effective config" endpoint exists yet.
- The `TC_` prefix with `__` nesting is less obvious than a flat environment variable scheme.
- Figment is a less common choice than `config-rs`, which may slow onboarding for developers familiar with the broader Rust ecosystem.

### Neutral
- `config.yaml` in the service directory is gitignored. Developers copy from `config.yaml.example` and modify locally.
- `Config::load_from(path)` exists for tests that need to skip the `/etc/tc/config.yaml` layer.
- The Helm chart's `values.yaml` defaults mirror the Rust struct defaults, creating two sources of truth for default values. Both are tested.

## Alternatives considered

### Single `DATABASE_URL` environment variable
- Standard approach (used by many ORMs and migration tools)
- Rejected because URL-encoding issues with special characters in passwords caused real bugs during testing. Individual fields are unambiguous.
- Also rejected because a single URL cannot be partially overridden (e.g., change only the host via environment variable while keeping port from config file).

### `config-rs` instead of Figment
- More popular crate with similar layered configuration
- Rejected because Figment's `Serialized::defaults()` integration with serde `#[serde(default)]` produces cleaner code — defaults live on the struct, not in a separate defaults map.

### Embed secrets in ConfigMap with restricted RBAC
- Simpler Helm chart (no Secret template)
- Rejected because ConfigMaps are not designed for secrets — they're not encrypted at rest by default, appear in `kubectl get configmap -o yaml`, and are logged in many Kubernetes audit configurations.

### No validation — rely on runtime errors
- Simpler startup code
- Rejected because a missing database password manifests as a cryptic connection timeout minutes after startup, not as a clear "password is required" error at boot.

## References
- [Figment documentation](https://docs.rs/figment)
- [Secure defaults policy](../interfaces/secure-defaults.md) — the policy this configuration implements
- [PR #313: Figment configuration](https://github.com/icook/tiny-congress/pull/313) — implementation
- `service/src/config.rs` — Config struct, load(), validate()
- `service/config.yaml.example` — documented example configuration
- `kube/app/templates/configmap.yaml` — Kubernetes ConfigMap template
- `kube/app/templates/secret.yaml` — Kubernetes Secret template
- `kube/app/templates/deployment.yaml` — environment variable injection
- `kube/app/values.yaml` — Helm chart defaults
