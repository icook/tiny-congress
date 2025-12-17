-- Accounts table for identity system MVP
-- Stores registered users with their root public key

CREATE TABLE IF NOT EXISTS accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    root_pubkey TEXT NOT NULL,  -- base64url encoded Ed25519 public key
    root_kid TEXT NOT NULL UNIQUE,  -- SHA-256 hash of pubkey, base64url, truncated to 16 bytes
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_accounts_username ON accounts(username);
CREATE INDEX idx_accounts_root_kid ON accounts(root_kid);
