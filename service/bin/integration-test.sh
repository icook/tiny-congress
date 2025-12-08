#!/bin/bash
set -e
DATABASE_URL=${DATABASE_URL:-postgres://postgres:postgres@postgres:5432/prioritization}
cd /usr/src/app
/usr/local/cargo/bin/cargo test --test integration_tests -- --test-threads=1 --nocapture
