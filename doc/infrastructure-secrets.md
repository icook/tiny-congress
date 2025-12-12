# Infrastructure Secrets Management

This document describes how to manage secrets in Kubernetes deployments for the Tiny Congress application.

## Required Secrets

The identity system requires the following secrets to be configured:

1. **SESSION_SIGNING_KEY** - JWT signing key for session tokens (see `doc/secrets-rotation.md`)
2. **DATABASE_URL** - PostgreSQL connection string (may include password)
3. **SESSION_SIGNING_KEY_OLD** (optional) - Previous signing key for zero-downtime rotation

## Kubernetes Secret Configuration

### Creating Secrets

**Option 1: kubectl create secret (recommended for production):**

```bash
# Generate a random signing key
SESSION_KEY=$(openssl rand -base64 32)

# Create secret from literal values
kubectl create secret generic tc-secrets \
  --from-literal=SESSION_SIGNING_KEY="$SESSION_KEY" \
  --from-literal=DATABASE_URL="postgres://user:pass@postgres:5432/tinycongress" \
  --namespace=default

# Verify secret was created
kubectl get secret tc-secrets -o yaml
```

**Option 2: Using a secret manifest (not recommended - easier to accidentally commit):**

```yaml
# secrets.yaml - DO NOT COMMIT THIS FILE
apiVersion: v1
kind: Secret
metadata:
  name: tc-secrets
type: Opaque
stringData:
  SESSION_SIGNING_KEY: "your-actual-secret-key-here"
  DATABASE_URL: "postgres://user:pass@postgres:5432/tinycongress"
```

Apply with:
```bash
kubectl apply -f secrets.yaml
rm secrets.yaml  # Delete immediately after applying
```

**Option 3: External Secrets Operator (recommended for production):**

For production deployments, use External Secrets Operator to sync from cloud secret managers:

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: tc-secrets
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: aws-secrets-manager  # or gcp-secret-manager, azure-key-vault
    kind: SecretStore
  target:
    name: tc-secrets
    creationPolicy: Owner
  data:
    - secretKey: SESSION_SIGNING_KEY
      remoteRef:
        key: tc/session-signing-key
    - secretKey: DATABASE_URL
      remoteRef:
        key: tc/database-url
```

### Updating Deployment to Use Secrets

Update `kube/app/templates/deployment.yaml` to reference the secret:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tc-api
spec:
  template:
    spec:
      containers:
        - name: api
          image: tc-api:latest
          env:
            - name: SESSION_SIGNING_KEY
              valueFrom:
                secretKeyRef:
                  name: tc-secrets
                  key: SESSION_SIGNING_KEY
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: tc-secrets
                  key: DATABASE_URL
            # For key rotation:
            - name: SESSION_SIGNING_KEY_OLD
              valueFrom:
                secretKeyRef:
                  name: tc-secrets
                  key: SESSION_SIGNING_KEY_OLD
                  optional: true  # Don't fail if not present
```

### Helm Values Configuration

Update `kube/app/values.yaml` to support secrets:

```yaml
# values.yaml
secrets:
  # If true, expects tc-secrets to exist in the namespace
  # If false, creates secret from values below (NOT RECOMMENDED for production)
  existingSecret: true
  secretName: tc-secrets

  # Only used if existingSecret is false (for development)
  sessionSigningKey: ""
  databaseUrl: ""
```

In `kube/app/templates/deployment.yaml`:

```yaml
env:
  - name: SESSION_SIGNING_KEY
    valueFrom:
      secretKeyRef:
        name: {{ .Values.secrets.secretName }}
        key: SESSION_SIGNING_KEY
  - name: DATABASE_URL
    valueFrom:
      secretKeyRef:
        name: {{ .Values.secrets.secretName }}
        key: DATABASE_URL
```

## Skaffold Configuration

Update `skaffold.yaml` to handle secrets in different profiles:

```yaml
profiles:
  - name: dev
    activation:
      - command: dev
    deploy:
      helm:
        releases:
          - name: tc
            chartPath: kube/app
            setValues:
              secrets.existingSecret: false
              # For dev, we can generate a temporary key
            setValueTemplates:
              secrets.sessionSigningKey: "dev-key-$(openssl rand -base64 32)"
              secrets.databaseUrl: "postgres://postgres:postgres@postgres:5432/tinycongress"

  - name: ci
    activation:
      - command: verify
    deploy:
      helm:
        releases:
          - name: tc
            chartPath: kube/app
            setValues:
              secrets.existingSecret: false
            setValueTemplates:
              secrets.sessionSigningKey: "ci-key-$(openssl rand -base64 32)"

  - name: production
    activation:
      - command: run
    deploy:
      helm:
        releases:
          - name: tc
            chartPath: kube/app
            setValues:
              secrets.existingSecret: true
              secrets.secretName: tc-secrets
```

## Security Best Practices

### DO:
- ✅ Use Kubernetes secrets for all sensitive values
- ✅ Use External Secrets Operator for production
- ✅ Rotate SESSION_SIGNING_KEY regularly (see `doc/secrets-rotation.md`)
- ✅ Enable encryption at rest for etcd (where secrets are stored)
- ✅ Use RBAC to restrict secret access
- ✅ Audit secret access via Kubernetes audit logs

### DON'T:
- ❌ Commit secrets to git (even encrypted)
- ❌ Put secrets in ConfigMaps (they're not encrypted)
- ❌ Use plain environment variables in deployment YAML
- ❌ Share production secrets between environments
- ❌ Log secret values (even in debug mode)

## Troubleshooting

### Secret not found error

```bash
# Check if secret exists
kubectl get secret tc-secrets

# Describe secret (doesn't show values)
kubectl describe secret tc-secrets

# View secret values (use carefully)
kubectl get secret tc-secrets -o jsonpath='{.data.SESSION_SIGNING_KEY}' | base64 -d
```

### Pod can't access secret

```bash
# Check pod logs
kubectl logs deployment/tc-api

# Check pod environment
kubectl exec deployment/tc-api -- env | grep SESSION

# Verify RBAC permissions
kubectl auth can-i get secrets --as=system:serviceaccount:default:tc-api
```

### Rotation not working

See `doc/secrets-rotation.md` for detailed rotation procedures. Common issues:
- Old key not set correctly in SESSION_SIGNING_KEY_OLD
- Pods not restarted after secret update
- Token TTL hasn't expired yet

## References

- [Kubernetes Secrets Documentation](https://kubernetes.io/docs/concepts/configuration/secret/)
- [External Secrets Operator](https://external-secrets.io/)
- Session key rotation: `doc/secrets-rotation.md`
- Backup procedures: `doc/skills/backup.md`
