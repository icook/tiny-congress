#!/bin/bash
set -e
cd /usr/src/app
/usr/local/cargo/bin/cargo test --test api_tests --test graphql_tests --test model_tests -- --test-threads=1 --nocapture
