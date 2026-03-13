# Architecture Overview

A visual guide to TinyCongress's major components and data flow. For entity
details and invariants, see [domain-model.md](domain-model.md). For the
three-layer backend pattern, see
[ADR-016](decisions/016-repo-service-http-architecture.md).

## System Diagram

```mermaid
graph TB
    subgraph Browser["Browser (React + Vite + Mantine)"]
        UI[UI Components]
        CryptoProvider[CryptoProvider<br/><i>lazy WASM init</i>]
        DeviceProvider[DeviceProvider<br/><i>IndexedDB key storage</i>]
        SignedFetch[signedFetchJson<br/><i>SubtleCrypto Ed25519 signing</i>]
        WASM["tc-crypto (WASM)<br/><i>derive_kid, base64url</i>"]
        Noble["@noble/curves<br/><i>Ed25519 key generation</i>"]
    end

    subgraph API["tinycongress-api (Axum)"]
        Router[Axum Router]
        Auth["AuthenticatedDevice extractor<br/><i>signature verification</i>"]
        CryptoNative["tc-crypto (native)<br/><i>verify_ed25519</i>"]

        subgraph Modules["Domain Modules (HTTP → Service → Repo)"]
            Identity["identity<br/><i>/auth/*</i>"]
            Reputation["reputation<br/><i>/me/endorsements, /verifiers/*</i>"]
            Rooms["rooms<br/><i>/rooms/*, /polls/*, /vote</i>"]
            Trust["trust<br/><i>/trust/endorse, /revoke, /scores</i>"]
        end

        subgraph Background["Background Tasks (tokio::spawn)"]
            TrustWorker["TrustWorker<br/><i>polls trust_action_queue</i>"]
            TrustEngine["TrustEngine<br/><i>recursive CTE graph walk</i>"]
            NonceCleaner["Nonce Cleanup<br/><i>expired nonce deletion</i>"]
        end
    end

    subgraph External["External Services"]
        IDme["ID.me OAuth 2.0<br/><i>identity verification</i>"]
        OpenRouter["OpenRouter LLM API<br/><i>room/poll generation</i>"]
    end

    subgraph Infra["Infrastructure"]
        PG[(PostgreSQL<br/><i>sqlx + migrations</i>)]
        Prom["/metrics<br/><i>Prometheus</i>"]
    end

    subgraph Tools["Standalone Binaries"]
        Sim["tc-sim<br/><i>simulation worker</i>"]
        Verifier["demo_verifier<br/><i>bootstrap tool</i>"]
        ExportOA["export_openapi"]
        ExportGQL["export_schema"]
    end

    %% Browser internals
    UI --> CryptoProvider
    UI --> DeviceProvider
    UI --> SignedFetch
    CryptoProvider --> WASM
    SignedFetch --> WASM

    %% Browser to API
    SignedFetch -- "REST API<br/>X-Device-Kid, X-Signature,<br/>X-Timestamp, X-Nonce" --> Router

    %% API internals
    Router --> Auth
    Auth --> CryptoNative
    Auth --> Modules
    Identity --> PG
    Reputation --> PG
    Rooms --> PG
    Trust --> PG

    %% Trust async flow
    Trust -- "enqueues action<br/>(202 Accepted)" --> TrustWorker
    TrustWorker --> TrustEngine
    TrustEngine -- "recompute scores<br/>(recursive CTE)" --> PG
    NonceCleaner --> PG

    %% External
    Reputation -- "OAuth callback" --> IDme
    Sim -- "signed REST calls" --> Router
    Sim --> OpenRouter

    %% Tools
    Verifier -- "bootstrap accounts" --> Router
    ExportOA -. "dumps openapi.json" .-> API
    ExportGQL -. "dumps schema.graphql" .-> API

    %% Styling
    classDef browser fill:#e8f4f8,stroke:#2196F3,color:#000
    classDef api fill:#fff3e0,stroke:#FF9800,color:#000
    classDef crypto fill:#fce4ec,stroke:#E91E63,color:#000
    classDef infra fill:#e8f5e9,stroke:#4CAF50,color:#000
    classDef external fill:#f3e5f5,stroke:#9C27B0,color:#000
    classDef tools fill:#eceff1,stroke:#607D8B,color:#000

    class UI,CryptoProvider,DeviceProvider,SignedFetch browser
    class WASM,Noble,CryptoNative crypto
    class Router,Auth,Identity,Reputation,Rooms,Trust,TrustWorker,TrustEngine,NonceCleaner api
    class PG,Prom infra
    class IDme,OpenRouter external
    class Sim,Verifier,ExportOA,ExportGQL tools
```

## Crypto Boundary

The server is a **witness**, not an authority. Private key material never
leaves the browser.

```mermaid
graph LR
    subgraph Client["Browser (trust boundary)"]
        KeyGen["Key generation<br/><i>@noble/curves</i>"]
        Signing["Request signing<br/><i>SubtleCrypto Ed25519</i>"]
        Encrypt["Backup encryption<br/><i>Argon2id + AES-GCM</i>"]
        WASM2["tc-crypto WASM<br/><i>KID derivation, base64url</i>"]
    end

    subgraph Server["API server (witness)"]
        Verify["Signature verification<br/><i>tc-crypto native</i>"]
        Parse["Envelope parsing<br/><i>structure validation only</i>"]
        Store["Store encrypted backup<br/><i>opaque ciphertext</i>"]
    end

    KeyGen -- "public key" --> Verify
    Signing -- "signed request" --> Verify
    Encrypt -- "encrypted envelope" --> Parse
    Parse --> Store

    style Client fill:#e8f4f8,stroke:#2196F3,color:#000
    style Server fill:#fff3e0,stroke:#FF9800,color:#000
```

## Request Flow

A typical authenticated request through the three-layer backend:

```mermaid
sequenceDiagram
    participant B as Browser
    participant R as Axum Router
    participant A as AuthenticatedDevice
    participant H as HTTP Handler
    participant S as Service
    participant Repo as PgRepo
    participant DB as PostgreSQL

    B->>R: POST /trust/endorse<br/>+ signature headers
    R->>A: Extract & verify
    A->>A: verify_ed25519(pubkey, canonical_msg, sig)
    A->>A: Check timestamp skew (±300s)
    A->>A: Check nonce uniqueness
    A-->>H: AuthenticatedDevice { kid, account_id }
    H->>S: endorse(validated_input)
    S->>S: Validate business rules
    S->>Repo: insert_trust_action(endorse)
    Repo->>DB: INSERT INTO trust_action_queue
    DB-->>Repo: OK
    Repo-->>S: action_id
    S-->>H: Ok(action_id)
    H-->>B: 202 Accepted

    Note over DB: Later...
    participant TW as TrustWorker
    participant TE as TrustEngine
    TW->>DB: claim_pending_actions()
    TW->>TE: process(endorse)
    TE->>DB: Recursive CTE → recompute scores
    TW->>DB: mark_complete(action_id)
```
