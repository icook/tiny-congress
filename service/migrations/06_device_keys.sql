-- Device keys for delegated signing
-- Each device gets its own Ed25519 keypair, certified by the root key.
-- Device keys are the daily workhorses; the root key stays locked away.

CREATE TABLE IF NOT EXISTS device_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    device_kid TEXT NOT NULL,
    device_pubkey TEXT NOT NULL,          -- base64url Ed25519 public key
    device_name TEXT NOT NULL,            -- user-provided device name
    certificate BYTEA NOT NULL,           -- root key's Ed25519 signature over canonical cert message
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_device_keys_kid UNIQUE (device_kid)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_account ON device_keys(account_id);
