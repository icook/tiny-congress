# Prioritization Room Demo

This project demonstrates a "Prioritization Room" implementation - a system that allows groups to collectively prioritize topics through pairwise comparisons.

## Features

- Round-based prioritization with configurable tempo
- Topic pairing and voting mechanism
- Elo-like ranking system to determine topic priorities
- GraphQL API for real-time interaction
- Message queue for event handling
- React-based web client

## Architecture

The demo uses:
- **Rust** with **Axum** for the web server
- **PostgreSQL** with **PGMQ** for message queuing
- **SQLx** for database interactions
- **Refinery** for database migrations
- **Async-GraphQL** for the API layer
- **React** for the web client

## Running the Demo

### Prerequisites

- Rust toolchain
- PostgreSQL with PGMQ extension installed
- Node.js and npm (for the React client)
- Docker and Docker Compose (for containerized development/testing)
- Skaffold (for Kubernetes deployment and integration testing)

### Setup

1. Create a PostgreSQL database:
```
createdb prioritization
```

2. Install PGMQ extension:
```sql
CREATE EXTENSION pgmq;
```

3. Set environment variables:
```
export DATABASE_URL=postgres://username:password@localhost/prioritization
export GOOGLE_CLIENT_ID=<google-oauth-client-id>
export GOOGLE_CLIENT_SECRET=<google-oauth-client-secret>
export GOOGLE_REDIRECT_URI=http://localhost:3000/api/oauth/google/callback
export JWT_SECRET=<jwt-signing-secret>
```
The Google values configure the OAuth client (PKCE-friendly) and the JWT secret will be used to sign short-lived session tokens.

4. Run the server:
```
cargo run
```

5. In a separate terminal, run the client (optional):
```
cargo run --bin client
```

6. For the web interface, navigate to the `client` directory and run:
```
npm install
npm start
```

### Running Integration Tests

Use Skaffold so tests execute inside the same container images we ship:

```bash
skaffold build --file-output artifacts.json
skaffold test --build-artifacts artifacts.json
```

This pair of commands builds the dev images, runs the backend unit/API/integration suites plus the frontend lint/type/test/build chain inside containers, and tears everything down when finished.

For full Kubernetes parity (including port-forwards and verify hooks) run:

```bash
skaffold verify -p ci
```

This profile mirrors CI by deploying to your local cluster before executing the integration tests.

### Development with Skaffold

For local development with Kubernetes:

```bash
skaffold dev -p dev
```

This sets up a development environment with:
1. Local PostgreSQL database
2. Hot-reloading of the Rust API via `cargo watch` when files change
3. Skaffold file sync so local edits land in the container instantly
4. Kubernetes deployment for realistic testing

If you need to fall back to the pre-built binary (e.g. for profiling), set `DISABLE_CARGO_WATCH=1` in the deployment.

## API Schema

The GraphQL API provides:

- Queries:
  - `currentRound`: Get the currently active round
  - `currentPairing`: Get the current topic pairing for a round
  - `topTopics`: Get the highest ranked topics

- Mutations:
  - `submitVote`: Submit a vote for one topic in a pairing

## Round Logic

1. Each round runs for a configurable period (default: 60 seconds)
2. At the start of each round, random pairings of topics are created
3. Users vote on which topic in each pair they consider more important
4. At the end of the round, votes are tallied and topic rankings are updated
5. Rankings use an Elo-like system where topics gain or lose points based on wins/losses

## Extending the Demo

Potential extensions include:
- Adding user authentication
- Supporting multiple simultaneous rooms with different configurations
- Adding more sophisticated ranking algorithms
- Implementing federation between rooms
- Adding visualization of ranking changes over time
