-- pgmq extension is created in docker-entrypoint-initdb.d/init-extensions.sql
-- This migration creates the bot task queue using pgmq functions.
SELECT pgmq.create('rooms__bot_tasks');
