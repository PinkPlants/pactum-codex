# Pactum Protocol — Backend Specification

> **Version:** 0.12.0 — Draft  
> **Security audit:** 2026-02-25 — all H and M findings addressed  
> **Runtime:** Rust (Tokio)  
> **Framework:** Axum 0.8.x  
> **Database:** PostgreSQL 16 (via SQLx 0.8.x)  
> **Transport:** REST + WebSocket

---

## Table of Contents

1. [Overview](#1-overview)
2. [Technology Stack](#2-technology-stack)
3. [Project Structure](#3-project-structure)
4. [Configuration & Environment](#4-configuration--environment)
5. [Database Schema](#5-database-schema)
6. [Middleware Stack](#6-middleware-stack)
7. [Authentication](#7-authentication)
8. [API Routes](#8-api-routes)
9. [Payment](#9-payment)
10. [WebSocket](#10-websocket)
11. [Core Services](#11-core-services)
12. [Notification Pipeline](#12-notification-pipeline)
13. [Docker Setup](#13-docker-setup)
14. [Cargo.toml](#14-cargotoml)
15. [Planned Future Work](#15-planned-future-work)

---

## 1. Overview

The Pactum backend is an Axum-based HTTP + WebSocket server. It acts as a **UX convenience layer** — it never holds signing authority over the on-chain program and is fully replaceable. All credential validity is anchored to Solana chain state.

**Responsibilities:**
- Document upload + dual-layer SHA-256 hash verification (§10.1 of on-chain spec)
- Solana transaction construction and partial signing — vault_keypair co-signs every transaction as fee payer and `vault_funder`; client adds the user wallet signature before submission
- PostgreSQL indexing of agreement parties for efficient queries
- Real-time event pipeline via Solana `logsSubscribe` WebSocket
- Email notifications via Resend
- JWT authentication (SIWS wallet-native + OAuth2 Google / Microsoft)
- Party invitation by email — resolves email to pubkey or sends signup invite
- Per-agreement fee collection — USDC, USDT, PYUSD (stablecoins via Solana Pay); payment enforced before transaction is built; on-chain program has no knowledge of payment
- Scheduled expiry worker: submits `expire_agreement` transactions for agreements past their signing deadline, fired once per agreement at exactly `expires_at` (not a polling loop)

**What the backend never does:**
- Sign transactions on behalf of users
- Store `content_hash` or signatures as source of truth
- Control credential validity

---

## 2. Technology Stack

| Layer | Crate | Version |
|---|---|---|
| Web framework | `axum` | 0.8.x |
| Async runtime | `tokio` | 1.x (features = full) |
| HTTP middleware | `tower-http` | 0.6.x |
| Rate limiting | `tower-governor` | 0.4.x |
| Database | `sqlx` | 0.8.x (postgres + runtime-tokio + tls-rustls) |
| Migrations | `sqlx-cli` | 0.8.x |
| JWT | `jsonwebtoken` | 9.x |
| OAuth2 | `oauth2` | 4.x |
| Encryption | `aes-gcm` | 0.10.x |
| Hashing | `sha2` | 0.10.x |
| Solana client | `solana-client` | 2.2.x |
| Solana SDK | `solana-sdk` | 2.2.x |
| Serialization | `serde` + `serde_json` | 1.x |
| Email | `resend-rs` | latest |
| Config | `config` | 0.14.x |
| Tracing | `tracing` + `tracing-subscriber` | 0.1.x |
| Env vars | `dotenvy` | 0.15.x |
| UUID | `uuid` | 1.x (features = v4) |
| Time | `chrono` | 0.4.x |
| HTTP client | `reqwest` | 0.12.x (features = json) |
| Multipart | `axum-multipart` (via `axum-extra`) | 0.9.x |

---

## 3. Project Structure

```
pactum-codex/
├── Cargo.toml
├── Cargo.lock
├── .env.example
├── docker-compose.yml
├── api/
│   └── Dockerfile
├── migrations/
│   ├── 001_user_accounts.sql
│   ├── 002_auth_wallet.sql
│   ├── 003_auth_oauth.sql
│   ├── 004_user_contacts.sql
│   ├── 005_agreement_parties.sql
│   ├── 006_notification_queue.sql
│   ├── 007_agreement_drafts.sql
│   ├── 008_party_invitations.sql
│   ├── 009_agreement_payments.sql
│   └── 010_user_agreement_counts.sql
└── src/
    ├── main.rs
    ├── config.rs
    ├── error.rs
    ├── state.rs               -- AppState (db pool, config, clients)
    ├── router.rs              -- route assembly + middleware stack
    ├── middleware/
    │   ├── auth.rs            -- JWT extractor (AuthUser)
    │   └── wallet_guard.rs    -- rejects tx routes if pubkey == None
    ├── handlers/
    │   ├── auth.rs            -- SIWS + OAuth (Google, Microsoft) handlers; Apple deferred
    │   ├── upload.rs          -- document upload + hash verification
    │   ├── agreement.rs       -- agreement CRUD + sign + revoke
    │   ├── draft.rs           -- agreement draft lifecycle (pre-chain)
    │   ├── invite.rs          -- party invitation accept flow
    │   ├── payment.rs         -- Stripe + Solana Pay payment initiation + webhook
    │   ├── user.rs            -- contacts, preferences, display_name
    │   └── ws.rs              -- WebSocket upgrade handler
    ├── services/
    │   ├── crypto.rs           -- AES-256-GCM encrypt/decrypt
    │   ├── hash.rs             -- SHA-256 helpers
    │   ├── solana.rs           -- RPC client, tx construction + validated partial signing
    │   ├── solana_pay.rs       -- stablecoin reference generation + confirmation polling + mint verification
    │   ├── refund.rs           -- calculate_refund_amount(), execute_refund() SPL transfer
    │   ├── keypair_security.rs -- ProtectedKeypair newtype; secret loading; startup pubkey validation
    │   ├── storage.rs          -- IPFS / Arweave upload
    │   ├── notification.rs     -- email + push dispatch
    │   └── metadata.rs         -- NFT metadata JSON generation
    └── workers/
        ├── event_listener.rs      -- Solana logsSubscribe WebSocket; triggers refund_if_eligible on cancel/expire
        ├── keeper.rs              -- treasury sweep + balance alerts + invitation cleanup
        ├── expiry_worker.rs       -- submits expire_agreement txs; polls PostgreSQL every 5 min
        ├── refund_worker.rs       -- polls refund_pending payments; executes SPL token refund transfers
        └── notification_worker.rs -- polls notification_queue
```

---

## 4. Configuration & Environment

```toml
# .env.example
DATABASE_URL=postgres://pactum:secret@postgres:5432/pactum
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
SOLANA_WS_URL=wss://api.mainnet-beta.solana.com

PROGRAM_ID=<pactum_program_pubkey>

JWT_SECRET=<random_256bit_hex>
JWT_ACCESS_EXPIRY_SECONDS=900        # 15 minutes — short window limits leaked token exposure
JWT_REFRESH_EXPIRY_SECONDS=604800    # 7 days — stored in PG, revocable on logout

ENCRYPTION_KEY=<random_256bit_hex>         # AES-256 key for PII
ENCRYPTION_INDEX_KEY=<random_256bit_hex>   # HMAC key for blind email index

# OAuth providers — all free except Apple ($99/yr Apple Developer Program)
GOOGLE_CLIENT_ID=<google_oauth_client_id>
GOOGLE_CLIENT_SECRET=<google_oauth_client_secret>
GOOGLE_REDIRECT_URI=https://api.pactum.app/auth/oauth/google/callback

MICROSOFT_CLIENT_ID=<azure_app_client_id>
MICROSOFT_CLIENT_SECRET=<azure_app_client_secret>
MICROSOFT_REDIRECT_URI=https://api.pactum.app/auth/oauth/microsoft/callback
MICROSOFT_TENANT=common                    # 'common' allows personal + work accounts

# Apple Sign-In — deferred to future version

RESEND_API_KEY=<resend_api_key>
EMAIL_FROM=noreply@pactum.app
INVITE_BASE_URL=https://app.pactum.app/invite   # Base URL for party invitation links
INVITE_EXPIRY_SECONDS=604800                    # 7 days — invitation link validity (must be < expires_in_secs)
INVITE_REMINDER_AFTER_SECONDS=259200            # 3 days — send reminder if no response by then

# Payment
PLATFORM_FEE_USD_CENTS=199                       # Per-agreement fee: $1.99 (stored as cents, no float)
PLATFORM_FEE_FREE_TIER=3                         # Lifetime free agreements per user
PLATFORM_NONREFUNDABLE_FEE_CENTS=10              # $0.10 kept on cancel/expire after upload

# Platform keypairs — TWO separate hot wallets with distinct roles and blast radii
# See §11.5 for security model, secret loading procedure, and rotation runbook.

# Vault keypair — pays gas for create_agreement / expire_agreement / sign_agreement
# Holds SOL only. No stablecoin ATAs owned by this key.
# Target float: 1–2 SOL (≈ 200–400 agreements). Top up daily from cold wallet.
PLATFORM_VAULT_PUBKEY=<vault_pubkey>             # stored separately for startup validation
PLATFORM_VAULT_KEYPAIR_PATH=/run/secrets/vault_keypair.json  # never raw base58 in env

# Treasury keypair — owns stablecoin ATAs; signs refund SPL transfers only
# Holds stablecoin float only. Does NOT hold SOL beyond dust for rent.
# Target float: $50 per token. Sweep excess daily to cold wallet.
PLATFORM_TREASURY_PUBKEY=<treasury_pubkey>       # stored separately for startup validation
PLATFORM_TREASURY_KEYPAIR_PATH=/run/secrets/treasury_keypair.json

# Hot wallet safety thresholds
VAULT_MIN_SOL_ALERT=0.5                          # alert ops when vault SOL drops below this
VAULT_MIN_SOL_CIRCUIT_BREAKER=0.1               # halt server if vault drops below this
VAULT_FUNDING_RATE_LIMIT_PER_HOUR=50            # circuit breaker: max create_agreement fundings per hour
TREASURY_MIN_USDC_ALERT=20000000                # alert when USDC ATA < $20 (6 decimals)
TREASURY_FLOAT_PER_TOKEN=50000000               # keep $50 per token in hot wallet; sweep rest
TREASURY_SWEEP_DEST=<cold_wallet_pubkey>         # cold wallet or Squads multisig address

# Supported stablecoins — all have 6 decimals; $1.99 = 1_990_000 base units
# Initialize one ATA per token before go-live (owner = PLATFORM_TREASURY_PUBKEY):
#   spl-token create-account <MINT> --owner <TREASURY_PUBKEY>
STABLECOIN_USDC_MINT=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v
STABLECOIN_USDC_ATA=<platform_usdc_ata>

STABLECOIN_USDT_MINT=Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB
STABLECOIN_USDT_ATA=<platform_usdt_ata>

STABLECOIN_PYUSD_MINT=2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo
STABLECOIN_PYUSD_ATA=<platform_pyusd_ata>

# SOL payment removed — stablecoins only for v0.1
# Credit card (Stripe) — deferred to future version
# STRIPE_SECRET_KEY=
# STRIPE_WEBHOOK_SECRET=

IPFS_API_URL=https://api.pinata.cloud          # or self-hosted node
IPFS_JWT=<pinata_jwt>
ARWEAVE_WALLET_PATH=./arweave-wallet.json      # Arweave upload keypair

SERVER_PORT=8080
SERVER_HOST=0.0.0.0
```

`AppState` (shared across all handlers via `axum::extract::State`):

```rust
/// Newtype wrapper that prevents keypair bytes from appearing in logs or debug output.
/// Solana's Keypair does not implement Debug/Display; this newtype makes that explicit
/// and adds a safe redacted display for any context where AppState might be printed.
pub struct ProtectedKeypair(pub Keypair);

impl std::fmt::Debug for ProtectedKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ProtectedKeypair([REDACTED])")
    }
}

impl std::fmt::Display for ProtectedKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db:               PgPool,
    pub config:           Arc<Config>,
    pub solana:           Arc<RpcClient>,
    /// vault_keypair: pays gas for create_agreement / expire_agreement / sign_agreement.
    /// Holds SOL only. Low float (~1–2 SOL). Blast radius: vault float only.
    pub vault_keypair:    Arc<ProtectedKeypair>,
    /// treasury_keypair: owns stablecoin ATAs; signs refund SPL transfers only.
    /// Holds stablecoins only. Swept daily. Blast radius: $50 float per token.
    pub treasury_keypair: Arc<ProtectedKeypair>,
    /// Per-user WebSocket channels — keyed by user_id.
    /// Events are routed directly to the recipient; no global broadcast.
    pub ws_channels:      Arc<DashMap<Uuid, broadcast::Sender<WsEvent>>>,
}
```

---

## 5. Database Schema

### 5.1 User Accounts

```sql
-- Core identity table; one row per user
CREATE TABLE user_accounts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    display_name TEXT,            -- optional; user-provided; no trust value on-chain
    created_at   BIGINT NOT NULL DEFAULT extract(epoch from now())
);
```

> `display_name` is intentionally optional and deferred to v0.2 UI. It carries no on-chain trust value — the meaningful identities are wallet pubkey (unforgeable) and OAuth-verified email. Add it when you have UI to surface it.

-- Wallet auth method
CREATE TABLE auth_wallet (
    user_id    UUID NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    pubkey     TEXT PRIMARY KEY,
    linked_at  BIGINT NOT NULL DEFAULT extract(epoch from now())
);

-- OAuth auth method — supports any provider via the `provider` column
-- Supported v0.1: 'google' | 'microsoft'
-- Planned: 'apple' (future version)
CREATE TABLE auth_oauth (
    user_id     UUID NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,         -- 'google' | 'microsoft' | 'apple'
    provider_id TEXT NOT NULL,         -- provider's stable user ID (sub / oid / sub)
    linked_at   BIGINT NOT NULL DEFAULT extract(epoch from now()),
    PRIMARY KEY (provider, provider_id)
);

CREATE INDEX idx_auth_wallet_user ON auth_wallet(user_id);
CREATE INDEX idx_auth_oauth_user  ON auth_oauth(user_id);
```

### 5.2 User Contacts (PII — encrypted at application layer)

```sql
CREATE TABLE user_contacts (
    user_id          UUID PRIMARY KEY REFERENCES user_accounts(id) ON DELETE CASCADE,
    -- Encrypted fields (AES-256-GCM ciphertext + nonce)
    email_enc        BYTEA,
    email_nonce      BYTEA,
    email_index      BYTEA,   -- HMAC blind index for exact-match lookup
    phone_enc        BYTEA,
    phone_nonce      BYTEA,
    push_token_enc   BYTEA,
    push_token_nonce BYTEA,
    updated_at       BIGINT NOT NULL DEFAULT extract(epoch from now())
);

-- Allows "does this email already exist?" lookup without decryption
CREATE INDEX idx_user_contacts_email_index ON user_contacts(email_index);
```

### 5.3 Agreement Party Index

```sql
-- One row per (party, agreement) pair — the primary reason SQL exists
CREATE TABLE agreement_parties (
    party_pubkey   TEXT    NOT NULL,
    agreement_pda  TEXT    NOT NULL,
    creator_pubkey TEXT    NOT NULL,
    status         TEXT    NOT NULL DEFAULT 'PendingSignatures',
    signed_at      BIGINT,           -- NULL until this party has signed
    created_at     BIGINT  NOT NULL,
    expires_at     BIGINT  NOT NULL,
    title          TEXT    NOT NULL,
    PRIMARY KEY (party_pubkey, agreement_pda)
);

CREATE INDEX idx_agreement_parties_pubkey  ON agreement_parties(party_pubkey);
CREATE INDEX idx_agreement_parties_status  ON agreement_parties(party_pubkey, status);
CREATE INDEX idx_agreement_parties_pda     ON agreement_parties(agreement_pda);
```

### 5.4 Notification Queue

```sql
CREATE TABLE notification_queue (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type       TEXT    NOT NULL,   -- 'AgreementCreated', 'Signed', etc.
    agreement_pda    TEXT    NOT NULL,
    recipient_pubkey TEXT    NOT NULL,
    status           TEXT    NOT NULL DEFAULT 'pending',  -- pending | sent | failed
    attempts         INT     NOT NULL DEFAULT 0,
    created_at       BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    scheduled_at     BIGINT  NOT NULL DEFAULT extract(epoch from now())
);

CREATE INDEX idx_notification_queue_pending
    ON notification_queue(status, scheduled_at)
    WHERE status = 'pending';
```

### 5.5 Agreement Drafts

Holds pre-chain agreement state while waiting for unregistered party pubkeys to resolve. A draft is created when at least one invited party email cannot be resolved to a known wallet. Once all pubkeys are resolved, the backend notifies the creator to sign and submit `create_agreement`. If the creator never returns, the draft can be discarded with no on-chain cleanup needed — no on-chain state has been created yet.

```sql
CREATE TABLE agreement_drafts (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    creator_pubkey   TEXT    NOT NULL,
    -- Stores title, parties, expires_in_secs only — NO document, NO storage_uri at this stage
    -- Written exclusively by backend handler — never raw user JSON.
    -- Deserialised via DraftPayload struct with #[serde(deny_unknown_fields)] (M-2).
    draft_payload    JSONB   NOT NULL,
    -- Tracks resolution status of each party slot
    -- e.g. [{"pubkey": "ABC..."}, {"pubkey": null, "invite_id": "uuid"}]
    -- email_hint is NOT stored here — only in party_invitations (PII isolation, L-4)
    party_slots      JSONB   NOT NULL,
    status           TEXT    NOT NULL DEFAULT 'awaiting_party_wallets',
    -- status: awaiting_party_wallets | ready_to_submit | submitted | discarded
    created_at       BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    ready_at         BIGINT,    -- set when all pubkeys resolved
    submitted_at     BIGINT     -- set when create_agreement confirmed on-chain
    -- NOTE: no document_enc or document_key fields — document is never stored here.
    -- Upload to Arweave/IPFS is deferred until POST /draft/{id}/submit,
    -- ensuring zero storage fees are incurred for discarded drafts.
);

CREATE INDEX idx_agreement_drafts_creator ON agreement_drafts(creator_pubkey);
CREATE INDEX idx_agreement_drafts_status  ON agreement_drafts(status);
```

**`DraftPayload` Rust struct (M-2 fix):**

All writes to `draft_payload` go through this typed struct. `#[serde(deny_unknown_fields)]` rejects any unexpected fields at deserialisation time — prevents malformed data from silently persisting and panicking on reads.

```rust
// src/models/draft.rs
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftPayload {
    pub title:           String,
    pub expires_in_secs: u32,
    // party entries contain pubkey only — email is never written to draft_payload
    // to keep PII isolated in party_invitations (encrypted)
    pub parties:         Vec<DraftPartyEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftPartyEntry {
    pub pubkey:    Option<String>,   // None until invitation accepted
    pub invite_id: Option<Uuid>,     // references party_invitations.id
}
```

### 5.6 Party Invitations

One row per unregistered party per draft. The invitation window is a **backend-only concept** — it has no on-chain significance. It exists solely to give the pre-chain draft state a clean lifecycle and prompt the creator if a party never responds.

The invitation window must always be shorter than `expires_in_secs` in the draft payload, enforced at `POST /agreement` time.

```sql
CREATE TABLE party_invitations (
    id                  UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    draft_id            UUID    NOT NULL REFERENCES agreement_drafts(id) ON DELETE CASCADE,
    invited_email_index BYTEA   NOT NULL,  -- HMAC blind index for lookup
    invited_email_enc   BYTEA   NOT NULL,  -- AES-256-GCM ciphertext
    invited_email_nonce BYTEA   NOT NULL,
    -- 32-byte CSPRNG hex-encoded to 64 characters — 256 bits of entropy (M-6 fix)
    token               TEXT    NOT NULL UNIQUE,
    status              TEXT    NOT NULL DEFAULT 'pending',
    -- status: pending | accepted | expired
    reminder_sent_at    BIGINT,
    reminder_count      INT     NOT NULL DEFAULT 0,
    created_at          BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    expires_at          BIGINT  NOT NULL
);

CREATE INDEX idx_party_invitations_token        ON party_invitations(token);
CREATE INDEX idx_party_invitations_draft        ON party_invitations(draft_id);
CREATE INDEX idx_party_invitations_email_index  ON party_invitations(invited_email_index);
CREATE INDEX idx_party_invitations_pending      ON party_invitations(status, expires_at)
    WHERE status = 'pending';
```

**Token generation (M-6 fix):**
```rust
let mut token_bytes = [0u8; 32];
OsRng.fill_bytes(&mut token_bytes);
let token = hex::encode(token_bytes);  // 64-char hex — 256 bits of CSPRNG entropy
```

**`GET /invite/{token}` response** — minimal information only; `email_hint` is never returned from an unauthenticated endpoint (M-6 fix):
```json
{
  "agreement_title": "Service Agreement",
  "creator_display":  "Alice",
  "expires_at":       1708473600
}
```

Rate limit on `GET /invite/{token}`: **5 req/min per IP** (stricter than the default API rate limit).

### 5.7 Payments

One row per agreement payment. Supports stablecoin payments via Solana Pay (USDC, USDT, PYUSD). Payment must be confirmed before `POST /draft/{id}/submit` is accepted.

```sql
CREATE TABLE agreement_payments (
    id               UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    draft_id         UUID    NOT NULL REFERENCES agreement_drafts(id) ON DELETE CASCADE,
    user_id          UUID    NOT NULL REFERENCES user_accounts(id),
    method           TEXT    NOT NULL,
    -- method: 'usdc' | 'usdt' | 'pyusd'
    status           TEXT    NOT NULL DEFAULT 'pending',
    -- status: pending | confirmed | refund_pending | refunded | failed

    -- USD amount charged
    usd_amount_cents INT     NOT NULL,  -- always 199 ($1.99)

    -- Stablecoin fields (all supported tokens have 6 decimals — amount always 1_990_000)
    token_reference_pubkey TEXT UNIQUE,  -- Solana Pay reference for tx identification
    token_mint             TEXT,         -- verified mint from StablecoinRegistry
    token_amount           BIGINT,       -- always 1_990_000 (1.99 × 10^6)
    token_tx_signature     TEXT,         -- confirmed Solana tx signature
    token_source_ata       TEXT,         -- platform treasury ATA used for this payment;
                                         -- stored at payment initiation for refund ATA validation (H-5)

    -- Refund fields (populated on cancel/expire)
    refund_amount          BIGINT,       -- token base units refunded; 0 if no refund
    refund_usd_cents       INT,          -- USD equivalent kept for accounting; = usd_amount_cents - nonrefundable
    refund_tx_signature    TEXT,         -- SPL transfer tx signature for the refund
    refund_initiated_at    BIGINT,
    refund_completed_at    BIGINT,

    created_at   BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    confirmed_at BIGINT
);

CREATE INDEX idx_agreement_payments_draft  ON agreement_payments(draft_id);
CREATE INDEX idx_agreement_payments_user   ON agreement_payments(user_id);
CREATE INDEX idx_agreement_payments_token  ON agreement_payments(token_reference_pubkey)
    WHERE token_reference_pubkey IS NOT NULL;
CREATE INDEX idx_agreement_payments_refund ON agreement_payments(status)
    WHERE status = 'refund_pending';  -- efficient scan for pending refund jobs
```

Also add `paid` flag and upload tracking to `agreement_drafts`:

```sql
ALTER TABLE agreement_drafts
    ADD COLUMN paid              BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN paid_at           BIGINT,
    ADD COLUMN payment_id        UUID REFERENCES agreement_payments(id),
    ADD COLUMN storage_uri       TEXT,      -- set after document uploaded to Arweave/IPFS
    ADD COLUMN storage_uploaded  BOOLEAN NOT NULL DEFAULT false;
    -- storage_uploaded = false → nothing spent yet → full refund on cancel/expire
    -- storage_uploaded = true  → Arweave/IPFS fee spent → partial refund ($1.89); $0.10 kept
```

Free tier usage is tracked per user:

```sql
CREATE TABLE user_agreement_counts (
    user_id           UUID PRIMARY KEY REFERENCES user_accounts(id) ON DELETE CASCADE,
    total_submitted   INT  NOT NULL DEFAULT 0,   -- incremented when create_agreement confirmed on-chain
    free_used         INT  NOT NULL DEFAULT 0    -- incremented for each free agreement consumed
);
```

A user is on the free tier as long as `free_used < PLATFORM_FEE_FREE_TIER (3)`. Once exhausted, every subsequent `POST /payment/initiate` requires payment.

---

## 6. Middleware Stack

Applied globally in this order:

```
Request
  │
  ▼  [CORS]             tower-http CorsLayer — whitelist frontend origins
  │
  ▼  [Rate Limiting]    tower-governor — per-IP, route-specific limits
  │
  ▼  [Tracing]          tower-http TraceLayer — structured request logs
  │
  ▼  [JWT Auth]         custom FromRequestParts extractor — optional on public routes
  │
  ▼  [Wallet Guard]     rejects transaction-building routes if JWT has no pubkey
  │
  ▼  Route Handler
```

```rust
// src/router.rs
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin([
            "https://pactum.app".parse().unwrap(),
            "https://app.pactum.app".parse().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    // Per-IP: 100 req/min general; tighter limits applied per route group
    let governor = GovernorLayer::new(
        GovernorConfigBuilder::default()
            .per_second(60)
            .burst_size(100)
            .finish()
            .unwrap(),
    );

    Router::new()
        .merge(auth_routes())
        .merge(upload_routes())
        .merge(agreement_routes())
        .merge(user_routes())
        .merge(ws_routes())
        .layer(cors)
        .layer(governor)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
```

---

## 7. Authentication

### 7.1 SIWS — Sign-In With Solana

Wallet-native login. No passwords. The challenge nonce is a short-lived UUID stored in PostgreSQL — not in-memory — ensuring correctness across multiple server instances and horizontal scaling.

```
GET  /auth/challenge           → { nonce: "uuid" }
POST /auth/verify              → { access_token, refresh_token }
     body: { pubkey, signature (base64), nonce }
```

**Nonce storage (`migrations/011_siws_nonces.sql`):**

```sql
CREATE TABLE siws_nonces (
    nonce      TEXT    PRIMARY KEY,
    created_at BIGINT  NOT NULL DEFAULT extract(epoch from now())
);
-- No explicit TTL column needed — keeper cleans up expired rows every 60s:
-- DELETE FROM siws_nonces WHERE created_at < extract(epoch from now()) - 300;
```

**Challenge issue:**
```rust
// GET /auth/challenge
let nonce = Uuid::new_v4().to_string();
sqlx::query!("INSERT INTO siws_nonces (nonce) VALUES ($1)", nonce)
    .execute(&db).await?;
Json(json!({ "nonce": nonce }))
```

**Verify logic:**
1. Consume nonce atomically — `DELETE ... WHERE nonce = $1 AND created_at > now() - 300 RETURNING nonce`; if no row returned → `InvalidOrExpiredNonce` (prevents replay attacks, multi-instance safe)
2. Verify ed25519 signature: `sig.verify(pubkey, nonce_bytes)`
3. Upsert `user_accounts` + `auth_wallet` (implicit signup on first login)
4. Return `{ access_token, refresh_token }` — see §7.5

```rust
// POST /auth/verify
let row = sqlx::query!(
    "DELETE FROM siws_nonces
     WHERE nonce = $1
       AND created_at > extract(epoch from now()) - 300
     RETURNING nonce",
    body.nonce
).fetch_optional(&db).await?;

if row.is_none() {
    return Err(AppError::InvalidOrExpiredNonce);
}
// continue with ed25519 verification...
```

> **Why PostgreSQL, not Redis?** The existing PG connection pool is sufficient for v0.1 nonce volume (one row insert per login attempt, deleted on use). Redis can be introduced in v0.2 as a performance optimisation if login throughput warrants it — the interface is identical.

### 7.2 OAuth2 — Google, Microsoft

Both follow the OAuth2 authorization code flow. Routes:

```
GET  /auth/oauth/google                    → redirect to Google consent screen
GET  /auth/oauth/google/callback           → exchange code → { access_token, refresh_token }

GET  /auth/oauth/microsoft                 → redirect to Microsoft consent screen
GET  /auth/oauth/microsoft/callback        → exchange code → { access_token, refresh_token }
```

**Provider comparison:**

| Provider | Cost | Notes |
|---|---|---|
| Google | Free | Simplest to set up |
| Microsoft | Free | Set `tenant=common` for personal + work accounts |
| Apple | Deferred | Future version — $99/yr Apple Developer Program; POST callback; ephemeral email |

**Callback logic (both providers):**
1. Exchange `code` for provider access token
2. Fetch user profile (`sub`/`oid`, `email`)
3. Check if `email` is already registered under a different `provider_id` — if so, prompt account linking rather than creating duplicate (M-5 fix — see §7.2 note below)
4. Upsert `user_accounts` + `auth_oauth` (implicit signup on first login)
5. Return `{ access_token, refresh_token }`

> **Cross-provider email conflict (M-5):** If `POST /auth/oauth/<provider>/callback` receives an email already associated with a different OAuth provider, the backend returns `409 EmailAlreadyRegistered` with a `{ existing_provider, link_url }` payload instead of creating a duplicate account. The frontend directs the user to log in with their original provider and use `POST /auth/link/oauth` to add the new provider to their account. This policy is enforced in the callback handler — never silently merged.

### 7.3 Link Wallet to OAuth Account

OAuth users sign up with email — they have no wallet and therefore no pubkey. To participate as a creator or signer in any agreement they must link a wallet client-side. This is a deliberate v0.1 constraint; v0.3 MPC wallets will remove it for parties.

```
POST /auth/link/wallet
     Authorization: Bearer <oauth_jwt>
     body: { pubkey, signature, nonce }
```

**Link flow:**
1. Frontend calls `GET /auth/challenge` to get a fresh nonce
2. User signs the nonce with their wallet (client-side — Phantom, Backpack, etc.)
3. `POST /auth/link/wallet` verifies ed25519 signature, stores pubkey in `auth_wallet`, updates JWT claims to include `pubkey`
4. All subsequent JWTs for this user carry `pubkey` — user can now act as creator or signer

**OAuth user capability matrix:**

| Capability | OAuth only (no wallet) | OAuth + linked wallet |
|---|---|---|
| Login | ✓ | ✓ |
| Receive email notifications | ✓ (email known from OAuth) | ✓ |
| Be invited as a party (by email) | ✓ — invitation sent | ✓ |
| Accept invitation + sign | ✗ — must link wallet first | ✓ |
| Create agreements | ✗ — wallet required | ✓ |
| Receive WS notifications | ✓ (if online) | ✓ |

> **Invitation email for wallet-less OAuth accounts:** When an invited party has a Pactum account but no linked wallet, the invitation email reads: *"You have a Pactum account but need to connect a Solana wallet to sign this agreement. [Connect Wallet]"* — distinct from the new-user invitation which prompts signup. The frontend detects `has_account = true, has_wallet = false` from `GET /invite/{token}` response and shows the wallet-linking flow directly.

> **v0.3 note:** MPC wallets (Privy / Magic.link) will derive a Solana keypair from the user's OAuth identity, removing the need to link an external wallet. The OAuth account becomes the signing identity directly. No on-chain program changes required.

### 7.4 JWT Extractor

```rust
// src/middleware/auth.rs
pub struct AuthUser {
    pub user_id: Uuid,
    pub pubkey:  Option<String>,   // None for OAuth-only users (no linked wallet)
}

// JWT claims
#[derive(Serialize, Deserialize)]
struct Claims {
    sub:    Uuid,
    pubkey: Option<String>,
    exp:    usize,
    iat:    usize,
    jti:    Uuid,   // unique token ID — used for logout blacklist if needed
}
```

**Wallet guard middleware:**

Routes marked `JWT + wallet` in the API table require `pubkey` to be present in the JWT. OAuth-only users without a linked wallet hit this guard and receive `403 WalletRequired` with a prompt to link their wallet.

```rust
pub struct AuthUserWithWallet {
    pub user_id: Uuid,
    pub pubkey:  String,   // guaranteed non-null — extractor rejects None
}

impl<S> FromRequestParts<S> for AuthUserWithWallet {
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        match auth.pubkey {
            Some(pubkey) => Ok(AuthUserWithWallet { user_id: auth.user_id, pubkey }),
            None => Err(AppError::WalletRequired {
                message: "This action requires a connected wallet. Please link a wallet to your account.",
                link_url: "/auth/link/wallet",
            }),
        }
    }
}
```

### 7.5 Token Refresh & Logout

**Access token / refresh token architecture (H-3 fix):**

Short-lived access tokens minimise the damage window if a token is leaked. Refresh tokens are stored in PostgreSQL and can be actively revoked on logout — providing true server-side session termination.

```
access_token:   15 minutes (stateless JWT — short window limits leak exposure)
refresh_token:  7 days     (stored in PostgreSQL — revocable on logout)
```

**Schema (`migrations/013_refresh_tokens.sql`):**

```sql
CREATE TABLE refresh_tokens (
    token_hash  TEXT    PRIMARY KEY,   -- SHA-256(token) — plaintext never stored
    user_id     UUID    NOT NULL REFERENCES user_accounts(id) ON DELETE CASCADE,
    created_at  BIGINT  NOT NULL DEFAULT extract(epoch from now()),
    expires_at  BIGINT  NOT NULL       -- created_at + 604800 (7 days)
);
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
```

**Routes:**

```
POST /auth/refresh    body: { refresh_token }  → { access_token }
POST /auth/logout     body: { refresh_token }  → 204
```

**Refresh flow:**
```rust
// POST /auth/refresh
let token_hash = sha256_hex(&body.refresh_token);
let row = sqlx::query!(
    "DELETE FROM refresh_tokens
     WHERE token_hash = $1
       AND expires_at > extract(epoch from now())
     RETURNING user_id",
    token_hash
).fetch_optional(&db).await?;

let user_id = row.ok_or(AppError::InvalidRefreshToken)?.user_id;

// Issue new access token + rotate refresh token (delete-on-use)
let new_access  = issue_access_token(user_id)?;
let new_refresh = issue_and_store_refresh_token(&db, user_id).await?;
Json(json!({ "access_token": new_access, "refresh_token": new_refresh }))
```

**Logout flow:**
```rust
// POST /auth/logout — actively revokes the refresh token
let token_hash = sha256_hex(&body.refresh_token);
sqlx::query!(
    "DELETE FROM refresh_tokens WHERE token_hash = $1",
    token_hash
).execute(&db).await?;
// access_token expires naturally within 15 minutes
```

> **Refresh token rotation:** Each `/auth/refresh` call deletes the old refresh token and issues a new one (delete-on-use). This detects token theft — if an attacker uses a stolen refresh token first, the legitimate client's next refresh will fail, alerting the user to re-authenticate. Keeper cleans up expired refresh tokens daily.

---

## 8. API Routes

### 8.1 Auth

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/auth/challenge` | — | Get SIWS nonce |
| `POST` | `/auth/verify` | — | Verify wallet signature → JWT |
| `GET` | `/auth/oauth/google` | — | Redirect to Google OAuth |
| `GET` | `/auth/oauth/google/callback` | — | OAuth code exchange → JWT |
| `GET` | `/auth/oauth/microsoft` | — | Redirect to Microsoft OAuth |
| `GET` | `/auth/oauth/microsoft/callback` | — | OAuth code exchange → JWT |
| `POST` | `/auth/link/wallet` | JWT (OAuth) | Link wallet to OAuth account |
| `POST` | `/auth/refresh` | — | Rotate refresh token → new access token |
| `POST` | `/auth/logout` | — | Revoke refresh token (server-side) |

### 8.2 Upload

Document upload is intentionally **deferred to Phase 2** — it only happens when the creator is ready to submit the agreement on-chain. This means:

- If invitations time out and the draft is discarded → **no upload ever happened → $0 storage fee**
- If all parties are already registered → upload happens immediately in the same session as `POST /agreement`
- If the agreement expires or is cancelled after upload → the small Arweave/IPFS fee (~$0.0008 per document) is accepted as a negligible cost of doing business

> **Arweave/IPFS fees are non-refundable by design** — permanent storage is the value proposition. The two-phase flow eliminates wasted fees for the most common failure case (unresolved invitations) at zero engineering cost.

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/upload` | JWT + wallet | Upload document; dual-layer hash verify; upload to IPFS/Arweave; only call when ready to submit |

**Request:** `multipart/form-data`
```
file:         <binary>
client_hash:  <hex string>   -- SHA-256 computed client-side before upload
backend:      "ipfs" | "arweave"
```

**Upload validation (M-1 fix):**

```rust
// src/handlers/upload.rs
const MAX_FILE_SIZE_BYTES: usize = 50 * 1024 * 1024;   // 50 MB hard limit
const ALLOWED_MIME_TYPES: &[&str] = &[
    "application/pdf",
    "image/png",
    "image/jpeg",
];

async fn upload_handler(/* ... */, mut multipart: Multipart) -> impl IntoResponse {
    while let Some(field) = multipart.next_field().await? {
        let content_type = field.content_type()
            .ok_or(AppError::MissingContentType)?
            .to_string();

        if !ALLOWED_MIME_TYPES.contains(&content_type.as_str()) {
            return Err(AppError::InvalidFileType);
        }

        // Read with size cap — reject immediately if exceeded
        let bytes = field
            .with_size_limit(MAX_FILE_SIZE_BYTES)
            .bytes()
            .await
            .map_err(|_| AppError::FileTooLarge)?;

        // ... hash verification and upload
    }
}
```

`MAX_FILE_SIZE_BYTES` and `ALLOWED_MIME_TYPES` are also enforced at the reverse proxy layer (Nginx `client_max_body_size 50m`) as a first line of defence before the request reaches the Rust handler.

**Response `200`:**
```json
{
  "storage_uri":     "ipfs://Qm...",
  "content_hash":    "abc123...",
  "storage_backend": "ipfs"
}
```

**Error responses:**
- `400 HashMismatch` — `client_hash` does not match server-computed SHA-256
- `422 UploadFailed` — IPFS/Arweave upload error

**Rate limit:** 10 req/min per IP.

> **Privacy note (v0.1):** Documents are stored in plaintext on Arweave/IPFS. Once uploaded, they are permanently public and cannot be revoked. Client-side encryption before upload is planned for v0.2. Until then, users should be informed via the privacy policy that uploaded documents are publicly accessible on a permanent decentralized network.

### 8.3 Agreement

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/agreement` | JWT + wallet | Build `create_agreement` transaction |
| `GET` | `/agreement/{pda}` | — | Fetch agreement state (from chain or cache) |
| `GET` | `/agreements` | JWT | List agreements for authenticated user |
| `POST` | `/agreement/{pda}/sign` | JWT + wallet | Build `sign_agreement` transaction |
| `POST` | `/agreement/{pda}/cancel` | JWT + wallet | Build `cancel_agreement` transaction |
| `POST` | `/agreement/{pda}/revoke` | JWT + wallet | Build `vote_revoke` transaction |
| `POST` | `/agreement/{pda}/retract` | JWT + wallet | Build `retract_revoke_vote` transaction |

### 8.4 Agreement Drafts (Pre-Chain)

Drafts exist only in the backend. They are created when at least one party email cannot be immediately resolved to a wallet pubkey. Once all pubkeys resolve, the creator is notified to sign and submit.

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/draft/{id}` | JWT | Get draft status and party slot resolution |
| `DELETE` | `/draft/{id}` | JWT (creator) | Discard draft — no on-chain action needed; no storage fee incurred |
| `PUT` | `/draft/{id}/reinvite` | JWT (creator) | Resend invitation to an expired party slot |
| `POST` | `/draft/{id}/submit` | JWT + wallet | Upload document + build `create_agreement` tx; only callable when draft status = `ready_to_submit` |

**`POST /agreement` — Full Resolution Flow:**

```
Creator submits POST /agreement (title + parties + expires_in_secs only)
        │
        ▼
For each party entry:
    pubkey provided → use directly
    email provided  → compute HMAC blind index
                    → look up in user_contacts
                        ├─ FOUND, has wallet
                        │       → resolve pubkey immediately
                        │
                        ├─ FOUND, no wallet (OAuth-only account)
                        │       → treat as unregistered for signing purposes
                        │       → create party_invitations row
                        │       → send wallet-linking invitation email
                        │         ("You have a Pactum account but need to connect
                        │           a wallet to sign this agreement.")
                        │
                        └─ NOT FOUND (new user)
                                → create party_invitations row
                                → send new-user invitation email
        │
        ▼
All pubkeys resolved immediately?
        │
        ├─ YES → prompt creator to upload document now
        │        → creator calls POST /upload → gets storage_uri + content_hash
        │        → backend builds create_agreement tx
        │        → return tx to creator to sign + submit
        │        { "status": "submitted", "transaction": "..." }
        │
        └─ NO  → create agreement_drafts row (NO document, NO upload, NO fee)
                 { "status": "awaiting_party_wallets",
                   "draft_id": "uuid",
                   "pending_invitations": [
                     { "email_hint": "b***@example.com", "invited_at": 1708473600 }
                   ]
                 }
                 Wait for invitations to resolve (see §8.5)
                 Document upload deferred until draft.status = ready_to_submit
```

> `email_hint` is a partially masked email (e.g. `b***@example.com`) so the creator can identify who hasn't responded without exposing the full address in the API response.

**Validation enforced at `POST /agreement`:**
```
INVITE_EXPIRY_SECONDS < expires_in_secs
```
The invitation window must be shorter than the signing window. If not, return `400 InviteWindowExceedsSigningWindow`.

**`GET /invite/{token}` response** includes account status so the frontend can show the correct flow:

```json
{
  "agreement_title": "Service Agreement",
  "creator_display":  "Alice",
  "expires_at":       1708473600,
  "has_account":      true,
  "has_wallet":       false
}
```

Frontend logic:
- `has_account: false` → show signup flow → wallet connect → accept
- `has_account: true, has_wallet: false` → show wallet-linking flow → accept
- `has_account: true, has_wallet: true` → show login → accept directly

### 8.5 Party Invitations

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/invite/{token}` | — | Validate token; return masked agreement preview |
| `POST` | `/invite/{token}/accept` | JWT + wallet | Accept invite; trigger draft re-check |

**`POST /invite/{token}/accept` flow:**

```
Invited party completes signup + links wallet
        │
        ▼
POST /invite/{token}/accept
        │
        ▼
Mark invitation status = 'accepted'
Store resolved pubkey in party_slots of agreement_drafts
        │
        ▼
Are ALL party slots now resolved for this draft?
        │
        ├─ NO (other invitations still pending)
        │       → save progress, wait
        │
        └─ YES (all pubkeys resolved)
                │
                ▼
        Mark draft status = 'ready_to_submit'
        Set draft.ready_at = now()
                │
                ▼
        Notify creator (online or offline):

        Online (active WS connection):
            → push WS event immediately
              { "event": "draft.ready_to_submit",
                "draft_id": "uuid",
                "message": "All parties have joined. Please submit your agreement." }

        Offline:
            → enqueue email notification
              Subject: "All parties have joined — your agreement is ready to submit"
            → enqueue push notification if token exists
```

**After creator is notified (draft.status = ready_to_submit):**

Creator opens app → reviews resolved party list → uploads document via `POST /upload` → calls `POST /draft/{id}/submit` with `storage_uri` + `content_hash` → backend builds `create_agreement` transaction → creator signs + submits to Solana RPC. The on-chain signing window (`expires_in_secs`) starts fresh from this moment.

> This is the earliest point at which Arweave/IPFS fees are incurred. If the draft was discarded before reaching this step, zero storage fees were spent.

**Invitation timeout handling (keeper job):**

```
Every 60 seconds, keeper scans party_invitations WHERE status = 'pending':

1. Reminder check:
   reminder_count = 0 AND created_at < now() - INVITE_REMINDER_AFTER_SECONDS
       → send reminder email
       → set reminder_sent_at = now(), reminder_count += 1

2. Expiry check:
   expires_at < now()
       → set status = 'expired'
       → notify creator:
         Subject: "A party hasn't responded to your agreement invitation"
         Body: "{email_hint} did not accept within 7 days.
                Options: [Resend Invitation] [Discard Draft]"

3. Resend invitation (creator action via PUT /draft/{id}/reinvite):
       → create new party_invitations row with fresh token + expires_at
       → send new invitation email
       → old expired row retained for audit
```

**Two-phase flow:**

**Phase 1 — `POST /agreement`** (no document required yet):
```json
{
  "title": "Service Agreement",
  "parties": [
    { "pubkey": "ABC123..." },
    { "email":  "alice@example.com" },
    { "email":  "bob@example.com" }
  ],
  "expires_in_secs": 2592000
}
```

No document, no `content_hash`, no `storage_uri` at this stage. The backend only needs enough information to resolve parties and send invitations.

**Phase 2 — `POST /draft/{id}/submit`** (all parties resolved, creator uploads document and signs):
```json
{
  "content_hash":    "abc123...",
  "storage_uri":     "ipfs://Qm...",
  "storage_backend": "ipfs"
}
```

Backend builds `create_agreement` transaction → returns unsigned tx to creator → creator signs + submits to Solana RPC.

> If all parties are already registered at `POST /agreement` time, Phase 1 and Phase 2 collapse into a single session — the creator uploads the document immediately and gets the transaction back in one flow without ever seeing the draft state.

**Response `200`** (all transaction-building endpoints):
```json
{
  "transaction":   "<base64 serialized unsigned transaction>",
  "agreement_pda": "XYZ..."
}
```

Client deserializes, signs with wallet, and submits to Solana RPC directly.

**`GET /agreements` query params:**
```
status=PendingSignatures|Completed|Cancelled|Expired|Revoked
role=creator|party|any          (default: any)
page=1
limit=20
```

### 8.5 User

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/user/me` | JWT | Get current user profile (id, display_name, linked auth methods) |
| `PUT` | `/user/me` | JWT | Update display_name |
| `PUT` | `/user/contacts` | JWT | Upsert encrypted email / phone / push token |
| `DELETE` | `/user/contacts` | JWT | Remove contact info |

**`PUT /user/me` — display_name validation (M-7 fix):**

`display_name` is embedded in HTML email notification templates. It must be sanitised before write to prevent stored XSS via malicious names like `<script>...</script>`.

```rust
// src/handlers/user.rs
const MAX_DISPLAY_NAME_LEN: usize = 64;

fn sanitise_display_name(name: &str) -> Result<String, AppError> {
    if name.len() > MAX_DISPLAY_NAME_LEN {
        return Err(AppError::DisplayNameTooLong);
    }
    // Reject HTML-significant characters outright — display_name has no
    // legitimate use for markup; rejection is cleaner than escaping
    if name.chars().any(|c| matches!(c, '<' | '>' | '"' | '\'' | '&')) {
        return Err(AppError::InvalidDisplayName);
    }
    Ok(name.trim().to_string())
}
```

All email templates must also HTML-escape `display_name` at render time as a defence-in-depth measure, even though the value has already been sanitised at write time.

**`PUT /user/contacts` request body:**
```json
{
  "email": "user@example.com",
  "phone": "+886912345678"
}
```

Contact fields are encrypted with AES-256-GCM before write. A blind HMAC index is stored alongside email for duplicate detection.

---

## 9. Payment

Per-agreement fee is enforced entirely at the backend API layer. The on-chain program is unchanged — it never sees the fee. Payment must be confirmed before `POST /draft/{id}/submit` builds the `create_agreement` transaction.

**Pricing:**
- First **3 agreements free** (lifetime per user)
- **$1.99 per agreement** thereafter
- Accepted payment methods: **USDC**, **USDT**, **PYUSD** — all stablecoins, always exactly $1.99
- Credit card (Stripe) deferred to a future version

> **SOL payment removed.** Stablecoins are strictly better for fixed-price payments — no price oracle, no rate lock, no tolerance math, no volatility exposure. SOL remains the gas token for Solana transactions but is not used as a payment method.


### 9.1 Routes

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/payment/initiate/{draft_id}` | JWT | Initiate payment — `method`: `usdc`, `usdt`, or `pyusd` |
| `GET` | `/payment/status/{draft_id}` | JWT | Poll payment confirmation status |

> **Credit card (Stripe)** deferred to a future version. **SOL payment** removed — stablecoins cover the use case without price oracle complexity.

### 9.2 Free Tier Check

Before any payment is initiated, the backend checks whether the user still has free agreements remaining:

```rust
async fn resolve_payment_requirement(
    db: &PgPool,
    user_id: Uuid,
) -> PaymentRequirement {
    let counts = get_user_agreement_counts(db, user_id).await;
    if counts.free_used < FREE_TIER_LIMIT {
        PaymentRequirement::Free
    } else {
        PaymentRequirement::Paid { usd_cents: PLATFORM_FEE_USD_CENTS }
    }
}
```

If `PaymentRequirement::Free`, `POST /draft/{id}/submit` sets `draft.paid = true` immediately and increments `free_used` without creating an `agreement_payments` row.

### 9.3 Payment Flow — Stablecoins (USDC / USDT / PYUSD)

All supported stablecoins share the same SPL token transfer mechanics — only the mint address and destination ATA differ. The amount is always exactly **1,990,000 base units** ($1.99 × 10⁶) since all three tokens have 6 decimal places. No price oracle needed.

**Supported stablecoin registry (`src/services/solana_pay.rs`):**

```rust
pub struct StablecoinInfo {
    pub symbol: &'static str,
    pub mint:   &'static str,   // loaded from env at startup
    pub ata:    &'static str,   // platform treasury ATA, loaded from env at startup
    pub decimals: u8,           // always 6 for all supported tokens
}

/// Registry is built at startup from env vars.
/// All mint addresses verified against Solana mainnet before go-live.
pub struct StablecoinRegistry {
    pub usdc:  StablecoinInfo,
    pub usdt:  StablecoinInfo,
    pub pyusd: StablecoinInfo,
}

impl StablecoinRegistry {
    /// Resolve a payment method string to its StablecoinInfo.
    /// Returns None for unknown or unsupported tokens.
    pub fn resolve(&self, method: &str) -> Option<&StablecoinInfo> {
        match method {
            "usdc"  => Some(&self.usdc),
            "usdt"  => Some(&self.usdt),
            "pyusd" => Some(&self.pyusd),
            _       => None,
        }
    }
}
```

**One-time setup** — initialize a treasury ATA for each supported token before go-live:

```bash
spl-token create-account EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v --owner <TREASURY_PUBKEY>
spl-token create-account Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB  --owner <TREASURY_PUBKEY>
spl-token create-account 2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo --owner <TREASURY_PUBKEY>
# Save each resulting ATA address to env as STABLECOIN_<TOKEN>_ATA
```

**Payment flow (same for all three tokens):**

```
POST /payment/initiate/{draft_id}
body: { "method": "usdc" }   -- or "usdt" or "pyusd"
        │
        ▼
Backend checks free tier → fee required
Backend resolves method → StablecoinInfo { mint, ata, decimals }
Backend generates unique reference keypair
        │
        ▼
Response:
{
  "method":             "usdc",
  "token_mint":         "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
  "treasury_ata":       "PlatformUsdcAta",
  "amount_units":       1990000,
  "usd_equivalent":     1.99,
  "reference_pubkey":   "UniqueReferencePubkey",
  "solana_pay_url":     "solana:PlatformUsdcAta?amount=1.99&spl-token=EPjFWdd5...&reference=UniqueReferencePubkey&label=Pactum&memo=draft-uuid"
}
        │
        ▼
Frontend renders QR code from solana_pay_url
OR builds SPL token transfer tx directly (wallet-adapter)
Creator signs + submits token transfer from their wallet
  (transfers from creator's token ATA → platform treasury ATA)
        │
        ▼
Backend polls Solana RPC via getSignaturesForAddress(reference_pubkey)
(every 5 seconds, up to 15 minutes — see payment status machine below)
        │
        ▼
Transfer found on-chain:
    → parse token transfer from tx instruction data
    → verify token_mint == expected mint from StablecoinRegistry  ← mandatory
    → verify token_amount == 1_990_000 units
    → execute atomic confirmation write (see below)
        │
        ▼
Push WS event: { "event": "payment.confirmed", "draft_id": "uuid", "method": "usdc" }
Creator can now call POST /draft/{id}/submit
```

> **Mint verification is mandatory.** The backend must verify that the transferred token's mint matches the expected mint from `StablecoinRegistry` before confirming payment. Without this, a malicious user could craft a transaction paying with a worthless SPL token that happens to use the same reference pubkey.

> **Creator's token ATA:** The creator must hold an initialized ATA for the chosen token. Modern wallets (Phantom, Backpack, Solflare) handle ATA initialization automatically when the user holds any supported token. No special handling needed in the backend.

**Atomic confirmation write (H-1 fix):**

The confirmation is a single `UPDATE ... WHERE status = 'pending' RETURNING` — condition check and write are one atomic database operation. PostgreSQL row-locking ensures two concurrent polling cycles finding the same transaction cannot both commit a confirmation. The second update finds `status` already `'confirmed'` and returns zero rows — treated as a no-op.

```rust
// src/services/solana_pay.rs
pub async fn confirm_payment_atomic(
    db:            &PgPool,
    reference:     &str,
    tx_signature:  &str,
    token_mint:    &str,
    token_amount:  i64,
) -> Result<bool, AppError> {
    // Single atomic statement — no separate SELECT + UPDATE
    // Returns the updated row only if status was 'pending' at write time
    let row = sqlx::query!(
        "UPDATE agreement_payments
         SET status             = 'confirmed',
             token_tx_signature = $2,
             token_mint         = $3,
             token_amount       = $4,
             confirmed_at       = extract(epoch from now())
         WHERE token_reference_pubkey = $1
           AND status = 'pending'
         RETURNING id",
        reference, tx_signature, token_mint, token_amount
    )
    .fetch_optional(db)
    .await?;

    // None → already confirmed (duplicate polling cycle) — safe to ignore
    Ok(row.is_some())
}
```

`token_tx_signature` unique index as second defence layer — rejects any duplicate write even if the atomic UPDATE logic were somehow bypassed:

```sql
-- migrations/012_payment_tx_sig_unique.sql
CREATE UNIQUE INDEX idx_payments_tx_sig
    ON agreement_payments(token_tx_signature)
    WHERE token_tx_signature IS NOT NULL;
```

**Payment status machine:**

```
pending ──────────────────────────────► confirmed   (normal: tx found within 15 min)
pending ──── keeper (15 min timeout) ──► expired    (no tx found in time)
expired ──── late tx arrives ──────────► refund_pending  (full refund — see M-4)
confirmed ── cancel/expire on-chain ───► refund_pending  (partial refund per §9.5)
refund_pending ────────────────────────► refunded
```

> **v0.2 — Helius Webhook:** In v0.2 the primary confirmation path will switch from polling to a Helius webhook (`POST /webhook/helius/payment`). The atomic `confirm_payment_atomic()` function is reused unchanged — only the trigger source changes from "polling found tx" to "webhook received tx". The keeper reconciliation job (see §12.2) is retained in both versions as a fallback for webhook delivery failures.

### 9.4 Payment Gate at Submit

`POST /draft/{id}/submit` enforces three things in sequence: payment confirmed, draft ready, creator has email on file. The `storage_uploaded` flag is set **atomically after the upload succeeds** — this is the point of no return for refund eligibility.

```rust
async fn submit_draft(/* ... */) -> impl IntoResponse {
    let draft = get_draft(&state.db, draft_id).await?;

    // Gate 1: payment must be confirmed
    if !draft.paid {
        return Err(AppError::PaymentRequired {
            draft_id,
            initiate_url: format!("/payment/initiate/{draft_id}"),
        });
    }

    // Gate 2: draft must be ready (all party pubkeys resolved)
    if draft.status != DraftStatus::ReadyToSubmit {
        return Err(AppError::DraftNotReady);
    }

    // Gate 3: creator must have an email on file to receive notifications.
    // SIWS users who skipped email collection are prompted here — at the moment
    // it matters most. OAuth users always pass this gate (email known from login).
    let has_email = sqlx::query_scalar!(
        "SELECT EXISTS(
             SELECT 1 FROM user_contacts
             WHERE user_id = $1 AND email_enc IS NOT NULL
         )",
        auth.user_id
    ).fetch_one(&state.db).await?.unwrap_or(false);

    if !has_email {
        return Err(AppError::EmailRequired {
            message: "Add an email address to receive agreement notifications.",
            add_email_url: "/user/contacts",
        });
    }

    // Upload document to Arweave/IPFS — point of no return for full refund
    // After this line, the $0.10 non-refundable service fee applies on cancel/expire
    let storage_uri = upload_document(&state.storage, &body.file, &body.content_hash).await?;

    // Mark storage as uploaded atomically — refund amount changes here
    sqlx::query!(
        "UPDATE agreement_drafts
         SET storage_uri = $1, storage_uploaded = true
         WHERE id = $2",
        storage_uri, draft_id
    ).execute(&state.db).await?;

    // Build and return partially-signed create_agreement transaction
    // vault_keypair co-signs as fee payer and vault_funder
    let tx = build_create_agreement_tx(
        &state.solana, &state.vault_keypair, &draft, &storage_uri, &auth.pubkey
    ).await?;
    Ok(Json(json!({ "transaction": tx, "agreement_pda": draft.pda })))
}
```

**`POST /agreement/{pda}/sign` response** includes a `suggest_email` flag for SIWS users with no email on file. The frontend uses this to prompt email collection immediately after a successful sign — at the moment of highest engagement, with clear context. OAuth users always have email and never see this prompt.

```json
{
  "transaction":          "base64...",
  "suggest_email":        true,
  "suggest_email_reason": "Add your email to be notified when all parties sign and your credential is ready."
}
```
### 9.5 Refund Policy

Refund amount is determined by `storage_uploaded`. The $0.10 non-refundable service fee covers Arweave/IPFS permanent storage plus platform operational costs (notifications, RPC, compute) that are incurred at upload time and cannot be recovered.

| Scenario | `storage_uploaded` | Refund Amount | Platform Keeps |
|---|---|---|---|
| Draft discarded before submit | `false` | **$1.99** (full) | $0.00 — nothing spent |
| Creator abandons after payment, before upload | `false` | **$1.99** (full) | $0.00 — nothing spent |
| Agreement cancelled on-chain (`cancel_agreement`) | `true` | **$1.89** | $0.10 — storage + ops |
| Agreement expired on-chain (`expire_agreement`) | `true` | **$1.89** | $0.10 — storage + ops |
| NFT minted (`Completed`) | `true` | $0.00 | $1.99 |
| Paid, tx abandoned (edge case) | `true` | $0.00 | $1.99 |

**Refund calculation (`src/services/refund.rs`):**

```rust
/// Calculate refund amount in token base units.
/// Rounds down — platform never over-refunds.
///
/// Example: paid = 1_990_000, nonrefundable_cents = 10, total_cents = 199
///   → refund = 1_990_000 × (199 - 10) / 199 = 1_890_000 (≈ $1.89)
pub fn calculate_refund_amount(
    paid_units:           u64,  // e.g. 1_990_000
    nonrefundable_cents:  u32,  // PLATFORM_NONREFUNDABLE_FEE_CENTS = 10
    total_fee_cents:      u32,  // PLATFORM_FEE_USD_CENTS = 199
) -> u64 {
    paid_units * (total_fee_cents - nonrefundable_cents) as u64 / total_fee_cents as u64
}

/// Execute a stablecoin refund from the platform treasury ATA to the creator's ATA.
/// Uses the same token mint the creator originally paid with.
pub async fn execute_refund(
    rpc:       &RpcClient,
    treasury:  &Keypair,           // PLATFORM_TREASURY_KEYPAIR
    payment:   &AgreementPayment,
    amount:    u64,                // from calculate_refund_amount()
) -> Result<String, AppError> {
    let creator_ata = get_associated_token_address(
        &payment.creator_pubkey,
        &payment.token_mint.parse()?,
    );
    let treasury_ata = get_treasury_ata(&payment.token_mint)?;

    let ix = spl_token::instruction::transfer(
        &spl_token::id(),
        &treasury_ata,
        &creator_ata,
        &treasury.pubkey(),
        &[],
        amount,
    )?;

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&treasury.pubkey()),
        &[treasury],
        rpc.get_latest_blockhash().await?,
    );

    Ok(rpc.send_and_confirm_transaction(&tx).await?.to_string())
}
```

**Refund trigger — event listener (`src/workers/event_listener.rs`):**

Refunds are initiated **automatically** when `cancel_agreement` or `expire_agreement` confirms on-chain. No manual ops intervention required.

```rust
"CancelAgreement" | "ExpireAgreement" => {
    update_agreement_status(state, log).await;
    enqueue_notifications(state, NotificationEvent::AgreementCancelled, log).await;
    broadcast_ws(state, "agreement.cancelled", log);

    // Automatically initiate refund based on storage state
    initiate_refund_if_eligible(state, log).await;
}

async fn initiate_refund_if_eligible(state: &AppState, log: &ProgramLog) {
    let Some(payment) = get_payment_by_pda(&state.db, &log.agreement_pda).await else {
        return; // free tier — no payment to refund
    };

    let draft = get_draft_by_pda(&state.db, &log.agreement_pda).await;

    let refund_amount = if !draft.storage_uploaded {
        // Nothing spent — full refund
        payment.token_amount
    } else {
        // Arweave + ops incurred — partial refund, keep $0.10
        calculate_refund_amount(
            payment.token_amount,
            state.config.nonrefundable_fee_cents,
            state.config.platform_fee_cents,
        )
    };

    // Mark as pending and enqueue — executed by refund worker
    sqlx::query!(
        "UPDATE agreement_payments
         SET status = 'refund_pending',
             refund_amount = $1,
             refund_usd_cents = $2,
             refund_initiated_at = $3
         WHERE id = $4",
        refund_amount,
        (refund_amount * 100 / payment.token_amount as u64 * state.config.platform_fee_cents as u64 / 100) as i32,
        now(),
        payment.id
    ).execute(&state.db).await.ok();
}
```

**Refund worker (`src/workers/refund_worker.rs`):**

Polls `refund_pending` payments every 30 seconds and executes the SPL token transfer. Separate from the event listener so a failed refund tx does not block other event processing.

```rust
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;

        let pending = sqlx::query!(
            "SELECT * FROM agreement_payments WHERE status = 'refund_pending'",
        ).fetch_all(&state.db).await.unwrap_or_default();

        for payment in pending {
            match execute_refund(
                &state.solana,
                &state.treasury_keypair,
                &payment,
                payment.refund_amount as u64,
            ).await {
                Ok(sig) => {
                    sqlx::query!(
                        "UPDATE agreement_payments
                         SET status = 'refunded',
                             refund_tx_signature = $1,
                             refund_completed_at = $2
                         WHERE id = $3",
                        sig, now(), payment.id
                    ).execute(&state.db).await.ok();
                    // Notify creator via WS + email
                    enqueue_refund_notification(&state, &payment).await;
                }
                Err(e) => {
                    // Log and retry next cycle — creator ATA might be temporarily unavailable
                    tracing::error!("Refund failed for payment {}: {e}", payment.id);
                }
            }
        }
    }
}
```

> **Creator's ATA must exist.** The refund transfers tokens to the creator's ATA for the same mint they paid with. If the ATA no longer exists, the transfer will fail and retry. After 3 failed attempts within 24 hours the payment is flagged for manual ops review.

> **UI requirement:** Before `POST /draft/{id}/submit` is called, the frontend **must** display a clear disclosure:
> *"If your agreement is cancelled or expires before all parties sign, you will receive a $1.89 refund. A $0.10 service fee covers permanent document storage and cannot be refunded."*
> The user must explicitly confirm before the submit flow proceeds.

---

## 10. WebSocket

### 10.1 Endpoint

```
GET /ws
    Authorization: Bearer <jwt>   (via query param or header)
```

After upgrade, the server pushes real-time agreement status events to the connected client.

### 10.2 Event Schema

```json
{
  "event":          "agreement.signed",
  "agreement_pda":  "XYZ...",
  "data": {
    "signed_by":            "wallet_pubkey",
    "remaining_signatures": 1,
    "status":               "PendingSignatures"
  },
  "timestamp": 1708473600
}
```

**Event types:**

| Event | Trigger |
|---|---|
| `agreement.created` | `create_agreement` confirmed on-chain |
| `agreement.signed` | `sign_agreement` confirmed (partial) |
| `agreement.completed` | `sign_agreement` confirmed (final) |
| `agreement.cancelled` | `cancel_agreement` confirmed |
| `agreement.expired` | `expire_agreement` confirmed |
| `agreement.revoke_vote` | `vote_revoke` confirmed (partial) |
| `agreement.revoked` | `vote_revoke` confirmed (final) |
| `draft.ready_to_submit` | All party pubkeys resolved — creator must sign |
| `draft.invitation_expired` | An invited party did not respond in time |
| `payment.confirmed` | Payment confirmed (Stripe webhook or SOL on-chain) |

### 10.3 Broadcast Architecture

Events are routed to **per-user channels** — a `DashMap` keyed by `user_id`. Each WebSocket connection registers its sender on connect and deregisters on disconnect. The event publisher looks up only the relevant recipients by `user_id` before sending — no event ever reaches an unrelated connection.

```rust
// src/state.rs
use dashmap::DashMap;

// Per-user broadcast channels — keyed by user_id
// Each WS connection inserts its Sender on upgrade, removes it on close
pub type WsChannels = Arc<DashMap<Uuid, broadcast::Sender<WsEvent>>>;

#[derive(Clone)]
pub struct AppState {
    // ...
    pub ws_channels: WsChannels,
}

// Publishing an event — only reaches the intended recipient
pub fn send_to_user(state: &AppState, user_id: Uuid, event: WsEvent) {
    if let Some(tx) = state.ws_channels.get(&user_id) {
        tx.send(event).ok();  // ok() — user may have disconnected between lookup and send
    }
}

// For events with multiple recipients (e.g. all parties on an agreement)
pub fn send_to_users(state: &AppState, user_ids: &[Uuid], event: WsEvent) {
    for user_id in user_ids {
        send_to_user(state, *user_id, Arc::clone(&event));
    }
}

// WS upgrade handler
async fn ws_handler(state: AppState, auth: AuthUser, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| async move {
        let (tx, rx) = broadcast::channel(64);
        state.ws_channels.insert(auth.user_id, tx);

        handle_ws_connection(socket, rx).await;

        // Clean up on disconnect
        state.ws_channels.remove(&auth.user_id);
    })
}
```

> **Multiple sessions:** A user with two concurrent sessions (phone + desktop) will have the second connection overwrite the first in the `DashMap`. For v0.1 this is acceptable — single active session per user. Multi-session fan-out (one `Vec<Sender>` per user) can be added in v0.2 if needed.

> **Origin validation (L-1 fix):** The WS upgrade handler explicitly validates the `Origin` header against the configured allowlist before accepting the upgrade — standard CORS middleware does not cover WebSocket upgrades.

```rust
async fn ws_handler(/* ... */, headers: HeaderMap) -> Response {
    let origin = headers.get("origin").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !config.ws_allowed_origins.contains(origin) {
        return StatusCode::FORBIDDEN.into_response();
    }
    // proceed with upgrade
}
```

---

## 11. Core Services

### 11.1 Hash Verification (`src/services/hash.rs`)

```rust
pub fn compute_sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    Sha256::digest(data).into()
}

pub fn verify_client_hash(file_bytes: &[u8], client_hash_hex: &str) 
    -> Result<[u8; 32], AppError> 
{
    let server_hash = compute_sha256(file_bytes);
    let client_hash = hex::decode(client_hash_hex)
        .map_err(|_| AppError::InvalidHash)?;
    
    if server_hash.as_ref() != client_hash.as_slice() {
        return Err(AppError::HashMismatch);
    }
    Ok(server_hash)
}
```

### 11.2 Encryption (`src/services/crypto.rs`)

```rust
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit, OsRng, rand_core::RngCore}};

pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12]), AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| AppError::EncryptionFailed)?;
    Ok((ciphertext, nonce_bytes))
}

pub fn decrypt(ciphertext: &[u8], nonce: &[u8; 12], key: &[u8; 32]) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce);
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| AppError::DecryptionFailed)?;
    String::from_utf8(plaintext).map_err(|_| AppError::DecryptionFailed)
}

// Blind index for email lookup (HMAC-SHA256)
pub fn hmac_index(value: &str, key: &[u8; 32]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(value.as_bytes());
    mac.finalize().into_bytes().to_vec()
}
```

### 11.3 Transaction Construction (`src/services/solana.rs`)

The backend builds all transactions, **partially signs** them with `vault_keypair` as both fee payer and `vault_funder` co-signer, and returns them to the client. The client adds the required user wallet signature(s) and submits to the RPC. The backend never blindly signs — every transaction is validated before the platform keypair signs it.

**Account requirements by instruction:**

| Instruction | Platform signs as | User signs as | Additional signers |
|---|---|---|---|
| `create_agreement` | `vault_funder` + fee payer | `creator` | — |
| `sign_agreement` (partial) | `vault_funder` + fee payer | `signer` (party) | — |
| `sign_agreement` (final) | `vault_funder` + fee payer | `signer` (party) | `nft_asset` keypair (fresh) |
| `cancel_agreement` | `vault_funder` + fee payer | `creator` | — |
| `expire_agreement` | `vault_funder` + fee payer | — (platform only) | — |
| `vote_revoke` | `vault_funder` + fee payer | `voter` (party) | — |
| `retract_revoke_vote` | `vault_funder` + fee payer | `voter` (party) | — |

> **`sign_agreement` (final):** The backend must generate a fresh `nft_asset` keypair client-side and include it as a signer. MPL-Core's `CreateV2` requires the new asset account to sign the transaction, proving the address is unoccupied. The keypair is used once and then discarded — it has no ongoing significance. The `collection` account (derived from `agreement.collection`) must also be provided as an `UncheckedAccount`.

```rust
pub async fn build_create_agreement_tx(
    rpc:            &RpcClient,
    vault_keypair:  &ProtectedKeypair,
    draft:          &AgreementDraft,
    storage_uri:    &str,
    creator:        &Pubkey,
) -> Result<String, AppError> {
    // 1. Derive agreement PDA from creator + agreement_id
    let (agreement_pda, _) = derive_agreement_pda(creator, &draft.agreement_id);

    // 2. Look up creator's CollectionState PDA — required account on create_agreement
    let (collection_state_pda, _) = derive_collection_state_pda(creator);

    // 3. Build create_agreement instruction
    let create_ix = build_create_agreement_instruction(
        &agreement_pda,
        &collection_state_pda,
        &vault_keypair.0.pubkey(),
        creator,
        draft,
        storage_uri,
    );

    // 4. Validate before signing — never blindly sign
    validate_create_agreement_args(draft)?;

    // 5. Assemble with vault_keypair as fee payer; vault_keypair also signs as vault_funder
    let mut tx = Transaction::new_with_payer(
        &[create_ix],
        Some(&vault_keypair.0.pubkey()),
    );

    // 6. Platform partially signs — creator must add their signature client-side
    let blockhash = rpc.get_latest_blockhash().await?;
    tx.partial_sign(&[&vault_keypair.0], blockhash);

    Ok(base64::encode(bincode::serialize(&tx)?))
}

/// Validate create_agreement args before the platform signs.
fn validate_create_agreement_args(draft: &AgreementDraft) -> Result<(), AppError> {
    require!(draft.parties.contains(&draft.creator), AppError::CreatorNotInParties);
    require!(
        draft.parties.len() == draft.parties.iter().collect::<std::collections::HashSet<_>>().len(),
        AppError::DuplicateParty
    );
    require!(
        draft.expires_in_secs >= 1 && draft.expires_in_secs as i64 <= MAX_EXPIRY_SECONDS,
        AppError::ExpiryOutOfRange
    );
    Ok(())
}
```

**Refund transaction validation (`src/services/refund.rs`):**

Refund parameters are always sourced from the database — never from request input. This prevents any client manipulation of refund destination or amount.

```rust
pub async fn execute_refund(
    rpc:              &RpcClient,
    treasury_keypair: &ProtectedKeypair,
    payment:          &AgreementPayment,  // sourced from DB — not request
    config:           &Config,
) -> Result<String, AppError> {
    let refund_amount = payment.refund_amount
        .ok_or(AppError::NoRefundAmountSet)?;

    // Derive creator ATA from stored pubkey — never trust client-provided address
    let creator_ata = get_associated_token_address(
        &payment.creator_pubkey.parse::<Pubkey>()?,
        &payment.token_mint.parse::<Pubkey>()?,
    );

    // Derive expected treasury ATA from config registry and verify it matches
    // stored source ATA (H-5 fix)
    let expected_source_ata = get_treasury_ata(&payment.token_mint, config)?;
    require!(
        payment.token_source_ata == expected_source_ata.to_string(),
        AppError::TreasuryAtaMismatch
    );

    let ix = spl_token::instruction::transfer(
        &spl_token::id(),
        &expected_source_ata,
        &creator_ata,
        &treasury_keypair.0.pubkey(),
        &[],
        refund_amount as u64,
    )?;

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&treasury_keypair.0.pubkey()),
        &[&treasury_keypair.0],
        rpc.get_latest_blockhash().await?,
    );

    Ok(rpc.send_and_confirm_transaction(&tx).await?.to_string())
}
```

### 11.4 Metadata Generation (`src/services/metadata.rs`)

The backend generates the credential NFT metadata immediately before building the **final** `sign_agreement` transaction — only at that point is the full signed party list known. The flow is:

1. Generate metadata JSON for the agreement
2. Upload to Arweave/IPFS → receive permanent `metadata_uri`
3. Generate a fresh `nft_asset` keypair (required by MPL-Core `CreateV2` as a signer)
4. Build the `sign_agreement` transaction with `metadata_uri` in `SignAgreementArgs` and `nft_asset` keypair as an additional signer
5. Platform partially signs with `vault_keypair`; client adds the party signer signature and the `nft_asset` keypair signature

> **`nft_asset` keypair:** The on-chain program requires `nft_asset` to be a `Signer` on the final `sign_agreement` transaction. This proves the address is unoccupied (`lamports() == 0`) before MPL-Core allocates the account. The keypair is generated fresh per agreement, used once for this transaction, and then discarded — it has no ongoing significance.

```rust
pub fn build_metadata_json(agreement: &AgreementState) -> serde_json::Value {
    let id_short = hex::encode(&agreement.agreement_id[..4]);
    json!({
        "name": format!("Pactum #{} — {}", id_short, agreement.title),
        "description": "On-chain agreement credential issued via Pactum Protocol.",
        "image": format!("ar://{}", PACTUM_SEAL_TX_ID),
        "animation_url": format!("ar://{}", agreement.storage_uri),
        "external_url": format!("https://pactum.app/agreement/{}", agreement.pda),
        "attributes": build_attributes(agreement)
    })
}

/// Build the final sign_agreement transaction.
/// Called only when the invoking party is the last unsigned party.
pub async fn build_final_sign_agreement_tx(
    rpc:           &RpcClient,
    vault_keypair: &ProtectedKeypair,
    agreement:     &AgreementState,
    metadata_uri:  &str,
    signer:        &Pubkey,
) -> Result<(String, Keypair), AppError> {
    // Generate a fresh nft_asset keypair — used once, discarded after tx confirms
    let nft_asset_keypair = Keypair::new();

    let sign_ix = build_sign_agreement_instruction(
        &agreement.pda,
        &vault_keypair.0.pubkey(),
        signer,
        Some(&nft_asset_keypair.pubkey()),  // nft_asset (final signature only)
        Some(&agreement.collection),        // collection (final signature only)
        Some(metadata_uri.to_string()),
    );

    let mut tx = Transaction::new_with_payer(
        &[sign_ix],
        Some(&vault_keypair.0.pubkey()),
    );

    let blockhash = rpc.get_latest_blockhash().await?;
    // vault_keypair and nft_asset_keypair both sign here;
    // the party signer (signer) adds their signature client-side
    tx.partial_sign(&[&vault_keypair.0, &nft_asset_keypair], blockhash);

    // Return both the serialized tx and the nft_asset pubkey for reference
    Ok((base64::encode(bincode::serialize(&tx)?), nft_asset_keypair))
}
```

### 11.5 Platform Keypair Security (`src/services/keypair_security.rs`)

The platform operates two hot keypairs with distinct roles and blast radii. Neither keypair should ever be stored as a raw string in environment variables, config files, or logs.

**Threat model:**

| Threat | Vault keypair impact | Treasury keypair impact |
|---|---|---|
| Key leaked | Attacker drains SOL float (~1–2 SOL) | Attacker drains stablecoin float (~$150) |
| Server compromised | Same as above | Same as above |
| Malicious tx crafted | Limited by `validate_create_agreement_tx()` | Limited by DB-only refund params |

Blast radius is bounded by the float policy. Cold wallet and bulk funds are never exposed.

**Secret loading — never raw env vars:**

```rust
/// Load a keypair from a file path (preferred) or base58 env var (fallback).
/// The file should be mounted as a Docker secret or fetched from a secrets manager
/// at container startup — never baked into the image or committed to git.
pub fn load_keypair(path: &str) -> Result<ProtectedKeypair, AppError> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| AppError::KeypairLoadFailed(e.to_string()))?;

    // Solana keypair JSON is a [u8; 64] array
    let bytes: Vec<u8> = serde_json::from_str(&json)
        .map_err(|e| AppError::KeypairLoadFailed(e.to_string()))?;

    let keypair = Keypair::from_bytes(&bytes)
        .map_err(|e| AppError::KeypairLoadFailed(e.to_string()))?;

    Ok(ProtectedKeypair(keypair))
}

/// Called at server startup. Panics if pubkeys do not match config.
/// Catches wrong-file-loaded mistakes before any real transactions are signed.
pub fn validate_keypair_pubkeys(state: &AppState) {
    assert_eq!(
        state.vault_keypair.0.pubkey().to_string(),
        state.config.platform_vault_pubkey,
        "FATAL: vault keypair pubkey does not match PLATFORM_VAULT_PUBKEY — wrong key loaded"
    );
    assert_eq!(
        state.treasury_keypair.0.pubkey().to_string(),
        state.config.platform_treasury_pubkey,
        "FATAL: treasury keypair pubkey does not match PLATFORM_TREASURY_PUBKEY — wrong key loaded"
    );
    tracing::info!("Platform keypairs validated ✓");
}
```

**Secret storage options (in order of preference):**

| Option | How | Suitable for |
|---|---|---|
| **HashiCorp Vault** | AppRole auth; secret fetched at startup; held in memory only | Production VPS / self-hosted |
| **Docker secrets** | `docker secret create`; mounted at `/run/secrets/`; not in env | Docker Swarm / Compose |
| **Cloud secrets manager** | AWS Secrets Manager / GCP Secret Manager; IAM role auth | Cloud-hosted |
| **age-encrypted file** | `age -d keypair.age \| ./pactum-codex --keypair-stdin` | Dev / small deployments |

**Never:**
- Store raw base58 private key in `.env` or environment variables
- Commit keypair JSON files to git (add `*.json` to `.gitignore` for the keys directory)
- Log the keypair, AppState, or any struct containing `ProtectedKeypair`
- Reuse the same keypair for vault and treasury roles

**Key rotation procedure:**

1. Generate new keypair: `solana-keygen new -o new_vault_keypair.json`
2. Fund new keypair with SOL gas float from cold wallet
3. Update `PLATFORM_VAULT_KEYPAIR_PATH` and `PLATFORM_VAULT_PUBKEY` in secrets manager
4. Restart server — startup validation confirms new key loaded
5. Drain old keypair balance back to cold wallet
6. Securely delete old keypair file

---

## 12. Notification Pipeline

### 12.1 Event Listener Worker (`src/workers/event_listener.rs`)

```rust
// Subscribes to Solana program logs via WebSocket
// Parses Anchor instruction logs → updates PostgreSQL + broadcasts WsEvent

pub async fn run(state: AppState) {
    let ws_url = &state.config.solana_ws_url;
    loop {
        match connect_and_listen(ws_url, &state).await {
            Ok(_)  => {},
            Err(e) => {
                tracing::error!("Event listener disconnected: {e}; reconnecting in 5s");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn handle_confirmed_tx(log: &ProgramLog, state: &AppState) {
    match log.instruction.as_str() {
        "CreateAgreement" => {
            upsert_agreement_parties(state, log).await;
            enqueue_notifications(state, NotificationEvent::AgreementCreated, log).await;
            broadcast_ws(state, "agreement.created", log);
        }
        "SignAgreement" => { /* update signed_at, broadcast, notify */ }
        "CancelAgreement" | "ExpireAgreement" => {
            update_agreement_status(state, log).await;
            enqueue_notifications(state, NotificationEvent::AgreementCancelled, log).await;
            broadcast_ws(state, "agreement.cancelled", log);
            initiate_refund_if_eligible(state, log).await;  // automatic refund trigger
        }
        "VoteRevoke" => { /* update revoke votes, broadcast */ }
        _ => {}
    }
}
```

### 12.2 Keeper Job (`src/workers/keeper.rs`)

Runs every 60 seconds. Handles invitation lifecycle, hot wallet health, treasury sweep, payment reconciliation, and auth record cleanup. Agreement expiry is **not** handled here — see §12.3.

```rust
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;

        // Scan 1: send reminder emails for pending invitations
        send_invitation_reminders(&state).await;

        // Scan 2: expire stale invitations and notify creators
        expire_stale_invitations(&state).await;

        // Scan 3: check hot wallet balances — alert or circuit-break if low
        check_hot_wallet_balances(&state).await;

        // Scan 4: sweep excess treasury stablecoins to cold wallet (runs daily)
        sweep_treasury_excess(&state).await;

        // Scan 5: expire timed-out pending payments (M-4)
        expire_timed_out_payments(&state).await;

        // Scan 6: reconcile — catch any payments confirmed on-chain after polling window (M-4)
        reconcile_late_payments(&state).await;

        // Scan 7: clean up expired siws_nonces and refresh_tokens (H-2, H-3)
        cleanup_expired_auth_records(&state).await;
    }
}

/// Check vault SOL and treasury stablecoin balances.
/// Sends ops alert if below warning threshold.
/// Hard-stops the server if vault SOL falls below circuit breaker threshold.
async fn check_hot_wallet_balances(state: &AppState) {
    let vault_sol = state.solana
        .get_balance(&state.vault_keypair.0.pubkey()).await
        .unwrap_or(0);

    if vault_sol < lamports_from_sol(state.config.vault_min_sol_circuit_breaker) {
        tracing::error!(
            "CIRCUIT BREAKER: vault SOL {} below minimum {} — halting",
            vault_sol, state.config.vault_min_sol_circuit_breaker
        );
        send_ops_alert(&state, "CRITICAL: vault SOL circuit breaker triggered").await;
        std::process::exit(1);
    }

    if vault_sol < lamports_from_sol(state.config.vault_min_sol_alert) {
        tracing::warn!("Vault SOL balance low: {} lamports", vault_sol);
        send_ops_alert(&state, "WARNING: vault SOL balance low — top up required").await;
    }

    // Check treasury USDC (repeat pattern for USDT, PYUSD)
    let usdc_balance = get_ata_balance(&state.solana, &state.config.stablecoin_usdc_ata).await
        .unwrap_or(0);

    if usdc_balance < state.config.treasury_min_usdc_alert {
        tracing::warn!("Treasury USDC balance low: {} units", usdc_balance);
        send_ops_alert(&state, "WARNING: treasury USDC balance low").await;
    }
}

/// Sweep stablecoin balances above the float threshold to cold wallet.
/// Runs once per day (tracked via last_sweep_at in a config table).
async fn sweep_treasury_excess(state: &AppState) {
    if !should_sweep_today(&state.db).await { return; }

    for (mint, ata) in state.config.stablecoin_atas() {
        let balance = get_ata_balance(&state.solana, &ata).await.unwrap_or(0);
        let keep = state.config.treasury_float_per_token;

        if balance > keep {
            let sweep_amount = balance - keep;
            match spl_transfer_to_cold(
                &state.solana,
                &state.treasury_keypair,
                &ata,
                &state.config.treasury_sweep_dest,
                &mint,
                sweep_amount,
            ).await {
                Ok(sig) => tracing::info!("Treasury sweep {}: {} units → {}", mint, sweep_amount, sig),
                Err(e)  => tracing::error!("Treasury sweep failed for {}: {e}", mint),
            }
        }
    }

    mark_swept_today(&state.db).await;
}

/// Mark pending payments older than 15 minutes as expired.
async fn expire_timed_out_payments(state: &AppState) {
    sqlx::query!(
        "UPDATE agreement_payments
         SET status = 'expired'
         WHERE status = 'pending'
           AND created_at < extract(epoch from now()) - 900"
    ).execute(&state.db).await.ok();
}

/// Reconciliation scan — checks chain for payments that arrived after their polling window expired.
async fn reconcile_late_payments(state: &AppState) {
    let expired = sqlx::query!(
        "SELECT id, token_reference_pubkey, token_mint, token_amount
         FROM agreement_payments
         WHERE status = 'expired'
           AND created_at > extract(epoch from now()) - 3600"
    ).fetch_all(&state.db).await.unwrap_or_default();

    for payment in expired {
        if let Ok(Some(tx)) = check_reference_on_chain(
            &state.solana,
            &payment.token_reference_pubkey,
        ).await {
            sqlx::query!(
                "UPDATE agreement_payments
                 SET status              = 'refund_pending',
                     token_tx_signature  = $1,
                     refund_amount       = token_amount,
                     refund_initiated_at = extract(epoch from now())
                 WHERE id = $2
                   AND status = 'expired'",
                tx.signature, payment.id
            ).execute(&state.db).await.ok();

            tracing::info!("Reconciled late payment {} — full refund initiated", payment.id);
        }
    }
}

/// Clean up expired siws_nonces (> 5 min) and expired refresh_tokens.
async fn cleanup_expired_auth_records(state: &AppState) {
    sqlx::query!(
        "DELETE FROM siws_nonces WHERE created_at < extract(epoch from now()) - 300"
    ).execute(&state.db).await.ok();

    sqlx::query!(
        "DELETE FROM refresh_tokens WHERE expires_at < extract(epoch from now())"
    ).execute(&state.db).await.ok();
}

async fn send_invitation_reminders(state: &AppState) {
    let needs_reminder = sqlx::query!(
        "SELECT * FROM party_invitations
         WHERE status = 'pending'
           AND reminder_count = 0
           AND created_at < $1",
        now() - INVITE_REMINDER_AFTER_SECONDS
    )
    .fetch_all(&state.db).await.unwrap_or_default();

    for inv in needs_reminder {
        enqueue_reminder_email(&state, &inv).await;
        sqlx::query!(
            "UPDATE party_invitations
             SET reminder_sent_at = $1, reminder_count = reminder_count + 1
             WHERE id = $2",
            now(), inv.id
        ).execute(&state.db).await.ok();
    }
}

async fn expire_stale_invitations(state: &AppState) {
    let stale = sqlx::query!(
        "UPDATE party_invitations SET status = 'expired'
         WHERE status = 'pending' AND expires_at < $1
         RETURNING *",
        now()
    )
    .fetch_all(&state.db).await.unwrap_or_default();

    for inv in stale {
        enqueue_invitation_expired_notification(&state, &inv).await;
        broadcast_ws(&state, "draft.invitation_expired", &inv);
    }
}
```

### 12.3 Expiry Worker (`src/workers/expiry_worker.rs`)

Submits `expire_agreement` transactions for agreements whose signing deadline has passed. Rather than polling every N seconds (which wastes gas on empty scans), this worker queries PostgreSQL for agreements that **just became eligible** and fires once per agreement.

**Design principle:** the on-chain `expires_at` timestamp is the source of truth. The worker's only job is to notice when `now >= expires_at` and submit the transaction promptly. Since signing windows are measured in days, a scan interval of a few minutes introduces negligible delay while keeping gas costs proportional to actual expired agreements.

```rust
pub async fn run(state: AppState) {
    // Scan every 5 minutes — signing windows are day-scale so sub-minute
    // precision is unnecessary. 5 min keeps gas spend proportional to load.
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        expire_due_agreements(&state).await;
    }
}

async fn expire_due_agreements(state: &AppState) {
    // Atomic status transition to 'expiring' — only the instance that wins
    // this UPDATE will submit the transaction. Prevents duplicate submissions
    // across multiple server instances or consecutive scan cycles.
    let locked = sqlx::query!(
        "UPDATE agreement_parties
         SET status = 'expiring'
         WHERE status = 'PendingSignatures'
           AND expires_at < extract(epoch from now())
         RETURNING agreement_pda, creator_pubkey"
    )
    .fetch_all(&state.db).await.unwrap_or_default();

    for row in locked {
        match build_and_submit_expire_tx(&state, &row).await {
            Ok(_) => {
                tracing::info!("Submitted expire_agreement for {}", row.agreement_pda);
                // Status will be updated to 'Expired' by the event listener
                // when the transaction confirms on-chain.
            }
            Err(e) => {
                // Revert so the next scan cycle retries
                tracing::error!("expire_agreement failed for {}: {e}", row.agreement_pda);
                sqlx::query!(
                    "UPDATE agreement_parties SET status = 'PendingSignatures'
                     WHERE agreement_pda = $1 AND status = 'expiring'",
                    row.agreement_pda
                ).execute(&state.db).await.ok();
            }
        }
    }
}
```

> **Why PostgreSQL polling instead of a scheduled job per agreement?** A per-agreement scheduled job (e.g. Tokio `sleep` until `expires_at`) would fire with zero delay and zero wasted scans, but requires in-memory state that is lost on server restart. The PostgreSQL approach is stateless and restart-safe: on any restart, the worker catches up on missed expirations immediately on the first scan. For agreements with day-scale windows, the 5-minute scan interval is imperceptible.

> **Gas cost is proportional to load.** The worker only submits a transaction when a row actually transitions from `PendingSignatures` to `expiring`. An empty scan (no expired agreements) costs nothing. A busy platform with many expiring agreements pays gas proportionally — the correct economic behaviour for a gasless-to-users model.

### 12.4 Notification Worker (`src/workers/notification_worker.rs`)

Polls `notification_queue` every 5 seconds. Dispatches via email if the recipient has one on file; falls back to WS-only if not. SIWS users who have not yet added an email receive real-time WS notifications only — they will not receive email notifications until they add an email via `PUT /user/contacts`.

```rust
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let jobs = fetch_pending_jobs(&state.db, 10).await;
        for job in jobs {
            let contact = get_contact(&state.db, &job.recipient_user_id).await;
            match dispatch(&state, &job, &contact).await {
                Ok(_)  => mark_sent(&state.db, job.id).await,
                Err(_) => increment_attempts(&state.db, job.id).await,
            }
        }
    }
}

async fn dispatch(state: &AppState, job: &NotificationJob, contact: &Option<UserContact>) {
    // Always attempt WS delivery first (instant, zero cost)
    send_to_user(state, job.recipient_user_id, build_ws_event(job));

    match contact {
        Some(c) if c.email_enc.is_some() => {
            // Has email — send full email notification
            send_email(state, job, c).await;
        }
        _ => {
            // No email on file (SIWS user who skipped email collection)
            // WS delivery above is the only channel available
            // These users are prompted to add email at submit time (Gate 3)
            // and after signing (suggest_email flag)
            tracing::debug!(
                "No email for user {} — WS-only notification for {:?}",
                job.recipient_user_id, job.event_type
            );
        }
    }
}
```

### 12.5 Notification Event Types & Templates

| Event | Subject | Recipients |
|---|---|---|
| `AgreementCreated` | "You've been invited to sign an agreement" | All parties except creator |
| `Signed` (partial) | "Agreement partially signed" | Creator + unsigned parties |
| `Completed` | "Agreement fully signed — credential minted" | All parties |
| `Cancelled` | "Agreement cancelled" | All parties except creator |
| `Expired` | "Agreement expired unsigned" | All parties |
| `RevokeVote` | "A party voted to revoke the credential" | All parties |
| `Revoked` | "Credential revoked by unanimous consent" | All parties |
| `DraftReadyToSubmit` | "All parties have joined — your agreement is ready to submit" | Creator only |
| `InvitationExpired` | "A party hasn't responded to your agreement invitation" | Creator only |
| `InvitationReminder` | "Reminder: you've been invited to sign an agreement" | Invited party |
| `PaymentConfirmed` | "Payment confirmed — your agreement is ready to submit" | Creator only |
| `RefundInitiated` | "Your refund of $1.89 is being processed" | Creator only |
| `RefundCompleted` | "Your refund of $1.89 has been sent to your wallet" | Creator only |

---

## 13. Docker Setup

### `docker-compose.yml`

Sensitive values are passed via Docker secrets — never hardcoded in the compose file. The `db_password` secret must be created before first run:

```bash
echo "$(openssl rand -base64 32)" | docker secret create db_password -
```

```yaml
services:
  api:
    build:
      context: .
      dockerfile: api/Dockerfile
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://pactum@postgres:5432/pactum
      # DATABASE_URL reads password from PGPASSWORD env or pgpass file —
      # alternatively mount db_password secret and read in entrypoint
      SOLANA_RPC_URL: ${SOLANA_RPC_URL}
      SOLANA_WS_URL: ${SOLANA_WS_URL}
      JWT_SECRET: ${JWT_SECRET}
      ENCRYPTION_KEY: ${ENCRYPTION_KEY}
      ENCRYPTION_INDEX_KEY: ${ENCRYPTION_INDEX_KEY}
      RESEND_API_KEY: ${RESEND_API_KEY}
      GOOGLE_CLIENT_ID: ${GOOGLE_CLIENT_ID}
      GOOGLE_CLIENT_SECRET: ${GOOGLE_CLIENT_SECRET}
      MICROSOFT_CLIENT_ID: ${MICROSOFT_CLIENT_ID}
      MICROSOFT_CLIENT_SECRET: ${MICROSOFT_CLIENT_SECRET}
      # Apple Sign-In env vars — deferred to future version
    secrets:
      - db_password
      - vault_keypair
      - treasury_keypair
    depends_on:
      postgres:
        condition: service_healthy
    restart: unless-stopped

  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: pactum
      POSTGRES_PASSWORD_FILE: /run/secrets/db_password   # read from secret, not plaintext
      POSTGRES_DB: pactum
    secrets:
      - db_password
    volumes:
      - pg_data:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U pactum"]
      interval: 5s
      timeout: 5s
      retries: 5
    restart: unless-stopped
    # PostgreSQL is NOT exposed externally — only reachable within Docker network

secrets:
  db_password:
    external: true   # created via: docker secret create db_password -
  vault_keypair:
    external: true   # created via: docker secret create vault_keypair ./vault_keypair.json
  treasury_keypair:
    external: true   # created via: docker secret create treasury_keypair ./treasury_keypair.json

volumes:
  pg_data:
```

> **`.gitignore` requirement:** `*.json` files in the keys directory, `.env`, `docker-compose.override.yml`, and any file matching `*keypair*` must be listed in `.gitignore`. A pre-commit hook should scan for accidental secret commits.

### `api/Dockerfile`

```dockerfile
FROM rust:1.82-slim AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
# Cache dependencies layer
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pactum-codex /usr/local/bin/pactum-codex

EXPOSE 8080
CMD ["pactum-codex"]
```

---

## 14. Cargo.toml

```toml
[package]
name    = "pactum-codex"
version = "0.1.0"
edition = "2021"

[dependencies]
# Web framework
axum       = { version = "0.8", features = ["multipart", "ws"] }
axum-extra = { version = "0.9" }
tokio      = { version = "1",   features = ["full"] }
tower      = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }
tower-governor = "0.4"

# Database
sqlx = { version = "0.8", features = [
    "runtime-tokio",
    "tls-rustls-ring-webpki",
    "postgres",
    "uuid",
    "chrono",
    "migrate",
]}

# Serialization
serde      = { version = "1", features = ["derive"] }
serde_json = "1"

# Auth
jsonwebtoken = "9"
oauth2       = "4"

# Crypto
aes-gcm = "0.10"
sha2    = "0.10"
hmac    = "0.12"
hex     = "0.4"

# Solana
solana-client    = "2.2"
solana-sdk       = "2.2"
spl-token        = "4"                   # SPL token transfers for refunds + treasury sweep
spl-associated-token-account = "2"      # derive ATA addresses

# Utilities
uuid    = { version = "1", features = ["v4"] }
chrono  = { version = "0.4", features = ["serde"] }
dotenvy = "0.15"
config  = "0.14"
base64  = "0.22"
dashmap = "6"                    # per-user WebSocket channel registry

# HTTP client (OAuth, IPFS, Arweave)
reqwest = { version = "0.12", features = ["json", "multipart"] }

# Logging
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Email
resend-rs = "0.8"

# Credit card (Stripe) — deferred to future version
# stripe-rust = "0.12"

[profile.release]
opt-level = 3
lto       = true
codegen-units = 1
```

---

## 15. Planned Future Work

### UX Abstraction Roadmap

Pactum's UX evolves in four stages, each reducing the blockchain knowledge required from users. Critically, **none of these stages require on-chain program changes** — the program always deals in pubkeys and signatures, which are abstract enough that any identity layer can sit above them. All complexity is absorbed by the backend and frontend.

| Stage | Creator experience | Party experience | Audience |
|---|---|---|---|
| **v0.1** | Solana wallet + stablecoin payment | Solana wallet | Crypto-native users |
| **v0.2** | Solana wallet OR credit card | Solana wallet | Wider crypto + tech-savvy |
| **v0.3** | Credit card | Email link + OTP (no wallet) | Mainstream — parties need nothing |
| **v0.4** | Credit card + email sign-up | Email link + OTP | Fully mainstream — no crypto knowledge required |

> **On-chain transparency:** At every stage the on-chain program sees only valid pubkeys signing transactions. Whether that pubkey belongs to a hardware wallet, a browser extension, or a platform-managed MPC wallet derived from an email address is entirely invisible to the program. The trust-minimized on-chain layer is stable across all four stages.

---

### Feature Table

| Version | Category | Feature | On-chain change? | Description |
|---|---|---|---|---|
| v0.2 | **Payment** | Stripe credit card | No | Add `stripe-rust`, `POST /payment/webhook/stripe`, Stripe PaymentIntent flow. Method `'stripe'` in `agreement_payments.method`. Stripe refunds automated via Stripe API on cancel/expire — same pattern as stablecoin refunds. Unlocks creator payments without a stablecoin wallet. |
| v0.2 | **Payment** | Fee sustainability review | No | Quarterly ops review of `PLATFORM_FEE_USD_CENTS` against SOL price. Break-even at ~$995/SOL — not an immediate risk but monitor. Raise fee in $0.50 increments if sustained high SOL price compresses margin below acceptable threshold. |
| v0.2 | **Security** | Multisig upgrade authority | No | Transfer backend deployment authority to a Squads M-of-N multisig. |
| v0.2 | **Protocol** | Document encryption | No | Client-side AES-256 encryption before upload; key derived from creator wallet signature; platform never sees plaintext. Aligns with on-chain spec §11. |
| v0.3 | **Identity** | Email-based party signing (MPC wallets) | No | Parties receive an email invitation link and sign via OTP — no wallet required. Backend integrates with an embedded wallet provider (Magic.link, Privy, or Turnkey) to derive a deterministic Solana keypair from the party's verified email. The derived pubkey is registered in `user_accounts` at invitation time and passed into `create_agreement.parties` as normal. The program sees a valid pubkey signature — it never knows the key was MPC-derived. Sequencing: party must complete email verification before `create_agreement` is submitted so their pubkey is known. |
| v0.3 | **Protocol** | M-of-N threshold signing | **Yes** | Add `threshold: u8` to `AgreementState`; change completion check from `signed_by.len() == parties.len()` to `signed_by.len() >= threshold`. Requires on-chain program upgrade. |
| v0.3 | **Protocol** | Delegation | **Yes** | Allow a party to nominate a delegate pubkey for signing. Add `delegations: Vec<(Pubkey, Pubkey)>` to `AgreementState`; add `delegate_signing` instruction. Requires on-chain program upgrade. |
| v0.3 | **Product** | Volume bundles | No | Pre-purchase agreement credits (e.g. 10 for $14.99) for business users. Add `agreement_credits` table; deduct credit at submit instead of charging per-agreement. |
| v0.3 | **Product** | Telegram bot notifications | No | Telegram Bot API as free real-time alternative to email. User initiates `/start` with `@PactumBot` to receive `chat_id`; stored in `user_contacts`. |
| v0.3 | **Product** | Display name + avatar | No | Surface `display_name` in UI; allow optional avatar upload. No on-chain trust value — identities are wallet pubkey and OAuth-verified email. |
| v0.4 | **Identity** | Full email-based creator flow | No | Creator signs up with email, pays by card, signs with OTP. Platform manages a custodial or MPC wallet on their behalf. `agreement.creator` points to the platform-derived pubkey. Blockchain is entirely invisible to users who don't care about it. Unlocks fully mainstream adoption. |
| v0.4 | **Governance** | Upgrade timelock | No (Squads config) | Add 7-day timelock to the multisig upgrade path via Squads. Community has observation window before any upgrade executes. |
| v1.0 | **Governance** | On-chain governance or immutability | No | Transfer upgrade authority to Realms governance (token-weighted voting) or permanently revoke it (`--final`) once protocol is mature and multiply audited. |

---

*— End of Backend Specification —*
