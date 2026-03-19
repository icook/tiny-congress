-- Ensure pgmq extension is available (also created in docker-entrypoint-initdb.d
-- but migrations must be self-contained for test databases).
CREATE EXTENSION IF NOT EXISTS pgmq;
SELECT pgmq.create('rooms__bot_tasks');
