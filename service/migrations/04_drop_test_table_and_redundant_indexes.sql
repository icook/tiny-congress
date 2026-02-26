-- Remove test infrastructure from production schema.
-- The test_items table (migration 02) exists solely for integration tests
-- and should not be in production databases.
--
-- Also drops redundant explicit indexes on accounts.username and accounts.root_kid.
-- The UNIQUE constraints on these columns already create implicit B-tree indexes
-- (accounts_username_key and accounts_root_kid_key), making the explicit indexes
-- (idx_accounts_username and idx_accounts_root_kid) wasteful duplicates.

DROP TABLE IF EXISTS test_items;

DROP INDEX IF EXISTS idx_accounts_username;
DROP INDEX IF EXISTS idx_accounts_root_kid;
