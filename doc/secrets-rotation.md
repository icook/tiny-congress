# Secrets Management and Key Rotation

This document describes how to manage secrets and perform zero-downtime key rotation for the Tiny Congress identity system.

## Session Signing Keys

Session tokens are signed using HS256 (HMAC with SHA-256) JWTs. The signing key is read from the `SESSION_SIGNING_KEY` environment variable.

### Generating a Signing Key

For production, generate a cryptographically secure random key:

```bash
openssl rand -base64 32
```

For development, any string works, but use a proper random secret in production.

### Key Storage

**Production:**
- Use a cloud secret manager (AWS Secrets Manager, Google Secret Manager, Azure Key Vault, etc.)
- Never commit secrets to git or bake them into Docker images
- Ensure deployment manifests reference the secret, not plain environment variables

**Development:**
- Use `.env` files (excluded from git via `.gitignore`)
- See `service/.env.example` for template

## Zero-Downtime Key Rotation

The session signing system supports multiple keys simultaneously to enable zero-downtime rotation.

### Rotation Process

1. **Generate new key:**
   ```bash
   NEW_KEY=$(openssl rand -base64 32)
   ```

2. **Add old key as fallback:**
   - Set `SESSION_SIGNING_KEY_OLD` to the current `SESSION_SIGNING_KEY` value
   - This allows the system to still verify existing tokens

3. **Update current key:**
   - Set `SESSION_SIGNING_KEY` to the new key
   - New tokens will be signed with the new key
   - Old tokens will still verify using `SESSION_SIGNING_KEY_OLD`

4. **Deploy the changes:**
   - Rolling deployment will gradually pick up the new configuration
   - Both old and new tokens remain valid during rollout

5. **Wait for old tokens to expire:**
   - Session tokens have a configurable TTL (default: varies by auth flow)
   - Wait at least one full TTL period after deployment

6. **Remove old key:**
   - Remove `SESSION_SIGNING_KEY_OLD` from configuration
   - Deploy to clean up the fallback

### Example: Kubernetes Secret Rotation

```bash
# 1. Get current key
CURRENT_KEY=$(kubectl get secret tc-secrets -o jsonpath='{.data.SESSION_SIGNING_KEY}' | base64 -d)

# 2. Generate new key
NEW_KEY=$(openssl rand -base64 32)

# 3. Update secret with both keys
kubectl create secret generic tc-secrets \
  --from-literal=SESSION_SIGNING_KEY="$NEW_KEY" \
  --from-literal=SESSION_SIGNING_KEY_OLD="$CURRENT_KEY" \
  --dry-run=client -o yaml | kubectl apply -f -

# 4. Restart deployments to pick up new secret
kubectl rollout restart deployment/tc-api

# 5. Wait for old tokens to expire (e.g., 24 hours)
sleep 86400

# 6. Remove old key
kubectl create secret generic tc-secrets \
  --from-literal=SESSION_SIGNING_KEY="$NEW_KEY" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl rollout restart deployment/tc-api
```

### Rotation Schedule

Recommended rotation schedule:
- **Production**: Rotate every 90 days
- **Staging**: Rotate every 30 days (to validate the process)
- **Development**: As needed

Set calendar reminders or use automation (e.g., AWS Secrets Manager auto-rotation).

## Root and Device Keys

**Important:** Root and device keys are client-side keys and are never stored on the server.

- Only public keys are stored in the database
- Private keys exist only on user devices
- Logging is configured to scrub any private key material
- API handlers reject payloads containing private keys

### Logging Safeguards

The tracing and audit logging systems are configured to:
- Never log fields containing private keys
- Redact PEM blocks in request/response logs
- Only log public keys and key IDs (kids)

### Code Guardrails

Handlers validate that:
- Account creation only accepts public keys
- Device registration only accepts public keys
- Payloads are rejected if they contain `-----BEGIN PRIVATE KEY-----` or similar markers

## Secret Scanning

### Pre-commit Protection

Optional: Install `git-secrets` or `gitleaks` to prevent committing secrets:

```bash
# Using git-secrets
brew install git-secrets
git secrets --install
git secrets --register-aws

# Using gitleaks
brew install gitleaks
git config --local core.hooksPath .git/hooks/
```

### CI Scanning

CI pipeline includes:
- Pattern matching for PEM blocks
- Checks that `.env` files are not committed
- Validation that Docker images don't contain secrets

## Backup and Recovery

### Session Key Loss

If the session signing key is lost:
- All existing sessions become invalid
- Users must re-authenticate
- Generate a new key and update the configuration

To minimize impact:
- Back up the key in your secret manager
- Enable automatic backups in cloud secret managers
- Document the key location in your runbook

### Database Backups

Session signing keys are not stored in the database. For database backup procedures, see the PostgreSQL documentation in `service/README.md`.

## Monitoring

Monitor for:
- Failed token verifications (may indicate key mismatch)
- Expired tokens (normal, but spike may indicate rotation issues)
- Authentication failures after rotation

Use the observability metrics (see INF-03) to track:
- `auth.success` and `auth.failure` counters
- `auth.token_verification_failed` counter
- Session duration histograms
