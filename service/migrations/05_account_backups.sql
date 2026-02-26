-- Encrypted backup storage for root keys
-- Stores password-encrypted root private key blobs for account recovery.
-- The server never sees plaintext key material.
-- Envelope format is Argon2id-only (version 1).

CREATE TABLE IF NOT EXISTS account_backups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kid TEXT NOT NULL,                   -- denormalized from accounts.root_kid for join-free recovery lookup
    encrypted_backup BYTEA NOT NULL,     -- binary envelope: version + KDF params + salt + nonce + AES-256-GCM ciphertext
    salt BYTEA NOT NULL,                 -- KDF salt (extracted from envelope for indexing)
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_account_backups_account UNIQUE (account_id),
    CONSTRAINT uq_account_backups_kid UNIQUE (kid)
);
