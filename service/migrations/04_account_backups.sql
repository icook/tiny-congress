-- Encrypted backup storage for root keys
-- Separate table (not columns on accounts) for:
-- - No NULLs on accounts table (backup is optional)
-- - Clean domain separation
-- - Future extensibility (multiple backup methods, history)

CREATE TABLE IF NOT EXISTS account_backups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kid TEXT NOT NULL,  -- denormalized from accounts.root_kid for join-free lookup
    encrypted_backup BYTEA NOT NULL,  -- binary envelope: version + KDF params + nonce + ciphertext
    salt BYTEA NOT NULL,  -- KDF salt (16 bytes)
    kdf_algorithm TEXT NOT NULL CHECK (kdf_algorithm IN ('argon2id', 'pbkdf2')),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_account_backups_account UNIQUE (account_id),
    CONSTRAINT uq_account_backups_kid UNIQUE (kid)
);

-- Primary lookup path for recovery (by KID, no join needed)
CREATE INDEX idx_account_backups_kid ON account_backups(kid);

COMMENT ON TABLE account_backups IS 'Password-encrypted root key backups for account recovery';
COMMENT ON COLUMN account_backups.kid IS 'Key ID (denormalized from accounts.root_kid for join-free lookup)';
COMMENT ON COLUMN account_backups.encrypted_backup IS 'Binary envelope: version + KDF params + nonce + AES-256-GCM ciphertext';
COMMENT ON COLUMN account_backups.salt IS 'Random salt for KDF (16 bytes)';
COMMENT ON COLUMN account_backups.kdf_algorithm IS 'KDF used: argon2id (preferred) or pbkdf2 (fallback)';
