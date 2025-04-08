#!/bin/bash

# Set variables
DB_NAME="prioritization_test"
DB_USER="postgres"
DB_PASSWORD="postgres" 
DB_HOST="localhost"
DB_PORT="5432"
TEST_COMMAND="cargo test --test integration_tests -- --test-threads=1 --nocapture"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Setting up PostgreSQL for integration tests...${NC}"

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
  echo -e "${RED}Docker is not running. Please start Docker.${NC}"
  exit 1
fi

# Function to clean up on exit
cleanup() {
  echo -e "${BLUE}Cleaning up...${NC}"
  docker-compose -f docker-compose.test.yml down
}

# Register the cleanup function to run on exit
trap cleanup EXIT

# Run the tests with Docker Compose
echo -e "${BLUE}Starting PostgreSQL container...${NC}"
docker-compose -f docker-compose.test.yml up -d postgres

# Wait for PostgreSQL to be ready
echo -e "${BLUE}Waiting for PostgreSQL to be ready...${NC}"
for i in {1..30}; do
  if docker-compose -f docker-compose.test.yml exec postgres pg_isready -U postgres > /dev/null 2>&1; then
    echo -e "${GREEN}PostgreSQL is ready!${NC}"
    break
  fi
  
  if [ $i -eq 30 ]; then
    echo -e "${RED}Timed out waiting for PostgreSQL to start.${NC}"
    exit 1
  fi
  
  echo -n "."
  sleep 1
done

# Create the test database if it doesn't exist
echo -e "${BLUE}Creating test database...${NC}"
docker-compose -f docker-compose.test.yml exec postgres psql -U postgres -c "CREATE DATABASE ${DB_NAME}" || true

# Enable the PGMQ extension in the database
echo -e "${BLUE}Creating PGMQ extension...${NC}"
docker-compose -f docker-compose.test.yml exec postgres psql -U postgres -d ${DB_NAME} -c "CREATE EXTENSION IF NOT EXISTS pgmq;" || true

# Export the DATABASE_URL
export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}"
echo -e "${BLUE}DATABASE_URL=${DATABASE_URL}${NC}"

# Run the integration tests
echo -e "${BLUE}Running integration tests...${NC}"
${TEST_COMMAND}

# Check the result
if [ $? -eq 0 ]; then
  echo -e "${GREEN}Integration tests passed!${NC}"
else
  echo -e "${RED}Integration tests failed!${NC}"
  exit 1
fi