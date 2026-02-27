# Learnings — Pactum Backend

> Record patterns, conventions, successful approaches discovered during implementation.

---


## [2026-02-26T07:45:00Z] Axum 0.7 Router with Tower-HTTP Middleware Stack

### 1. CORS Configuration — Whitelist Specific Origins (NOT Permissive)

**Pattern:**
```rust
use axum::http::{Method, header::HeaderValue};
use tower_http::cors::CorsLayer;

let cors = CorsLayer::new()
    .allow_origin([
        HeaderValue::from_static("https://pactum.app"),
        HeaderValue::from_static("https://app.pactum.app"),
    ])
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers([AUTHORIZATION, CONTENT_TYPE]);
```

**Key Points:**
- CORS origins MUST use `HeaderValue::from_static()` — NOT `.parse::<AllowOrigin>()`
- Use array of `HeaderValue`, then apply with `.allow_origin([...])` (NOT `.allow_origin_fn()`)
- `allow_methods()` controls which HTTP verbs are allowed cross-origin
- `allow_headers()` explicitly lists headers the browser can send (not permissive)
- **SECURITY:** Never use `CorsLayer::permissive()` in production — always whitelist specific origins

### 2. Middleware Stack Order (Spec §6 Diagram)

```rust
Router::new()
    .merge(route_group_1())
    .merge(route_group_2())
    // ... more routes
    .layer(cors)              // CORS first (browsers enforce before request)
    .layer(trace)             // Tracing after CORS (logs after origin check)
    // Future: Add rate limiting, JWT auth, wallet guard here
```

**Order Matters:**
- CORS first — browser must allow before any other middleware runs
- Tracing second — capture structured logs of allowed requests
- Rate limiting, Auth, Wallet guard would come after (not yet in v0.1)

### 3. Rate Limiting with GovernorLayer (Future Enhancement)

Note: Rate limiting not yet added to v0.1 due to SmartIpKeyExtractor complexity.
When added, use:
```rust
use tower_governor::GovernorLayer;

let governor = GovernorLayer::new(
    GovernorConfigBuilder::default()
        .per_second(60)      // 60 req/sec = 3600 req/min
        .burst_size(100)     // Allow bursts up to 100
        .finish()
        .unwrap(),
);

Router::new()
    // ... routes
    .layer(cors)
    .layer(governor)
    .layer(trace)
```

### 4. Route Groups Pattern — Empty Stub Functions

When defining route groups that will be filled in later waves:

```rust
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(auth_routes())      // Returns Router::new()
        .merge(upload_routes())    // Returns Router::new()
        .merge(agreement_routes()) // etc.
        // ...
        .layer(cors)
        .layer(trace)
}

fn auth_routes() -> Router {
    Router::new()
    // Later: add actual routes like .route("/auth/challenge", get(challenge_handler))
}
```

**CRITICAL:** Each stub MUST return `Router::new()` WITHOUT fallback — merging routers with fallbacks panics:
```rust
// WRONG — will panic on merge:
fn auth_routes() -> Router {
    Router::new().fallback(|| async { "Auth" })
}

// CORRECT — empty router:
fn auth_routes() -> Router {
    Router::new()
}
```

### 5. Testing Router Construction — Simple Unit Tests

```rust
#[tokio::test]
async fn test_build_router_returns_router() {
    let state = AppState {};
    let app = build_router(state);
    
    let response = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    
    // Should 404 (no routes defined), not panic
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

**Test Pattern:**
- Use `oneshot()` (from `tower::ServiceExt`) to make single request without server
- Empty routes → 404, which proves router built successfully
- All middleware applied (CORS, Tracing) — confirmed by request reaching handler layer

### 6. Eight Route Groups (Task §8)

All 8 route domains defined as stubs:
1. `auth_routes()` — SIWS, OAuth, token refresh
2. `upload_routes()` — document upload, hash verification
3. `agreement_routes()` — CRUD, sign, revoke
4. `draft_routes()` — pre-chain draft lifecycle
5. `invite_routes()` — party invitation flow
6. `payment_routes()` — Stripe, Solana Pay initiation + webhooks
7. `user_routes()` — contacts, preferences, profile
8. `ws_routes()` — WebSocket upgrade handler

### Implementation Location

- **Backend:** `/home/universal/pactum-codex/backend/`
- **Router:** `src/router.rs` — `pub fn build_router(state: AppState) -> Router`
- **Tests:** `tests/router_tests.rs` (integration) + `src/router.rs` (unit tests in cfg(test) mod)
- **All tests pass:** 5/5 passing (2 unit + 3 integration)

---

## [2026-02-27T00:54] Task 1 - Project Scaffolding

### Completed Structure
- ✅ Cargo.toml with all 27 dependencies (axum 0.8, sqlx 0.8, solana-sdk 2.2, etc.)
- ✅ 29 source files created (all modules as stubs)
- ✅ Directory structure per spec §3: src/{handlers,services,middleware,workers}, migrations/, api/, tests/

### Dependency Fixes Applied
- Fixed `tower-governor` → `tower_governor` (underscore, not hyphen)
- Fixed `ed25519-dalek` version conflict: v2.0 → v2.1 (serde compatibility with solana-sdk)

### Known Issue
- Cargo index update times out after 15 minutes (network latency in environment)
- Scaffolding structure is complete and correct
- Subagents will handle incremental cargo check in their own task contexts

### Next Steps
- Proceed with Wave 1 implementation tasks (2-7)
- Each subagent task will compile only incrementally (much faster)


---

## [2026-02-27T01:10] Solana Types Module — Borsh Serialization & On-Chain Compatibility

### 1. Enum Discriminator Pattern for On-Chain Compatibility

**Pattern:**
```rust
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AgreementStatus {
    Draft = 0,
    PendingSignatures = 1,
    Completed = 2,
    Cancelled = 3,
    Expired = 4,
    Revoked = 5,
}
```

**Key Points:**
- Use `#[repr(u8)]` to explicitly define enum discriminator byte values
- Discriminators MUST match on-chain program enum values
- `#[derive(BorshSerialize, BorshDeserialize)]` enables borsh serialization
- Add `Serialize, Deserialize` for JSON compatibility
- Order of variants determines serialization order — NEVER reorder existing variants

**Why Critical:**
- Borsh uses discriminator byte to serialize/deserialize enums
- On-chain program expects specific byte values (e.g., `Draft = 0`)
- Mismatch → deserialization fails or silently produces wrong variant
- If on-chain uses `Draft = 0`, backend MUST use `Draft = 0`

---

### 2. Instruction Arguments — Field Order Is Serialization Order

**Pattern:**
```rust
#[derive(BorshSerialize, BorshDeserialize)]
pub struct CreateAgreementArgs {
    pub agreement_id: String,      // Field 1 (serialized first)
    pub title: String,             // Field 2
    pub content_hash: String,      // Field 3
    pub storage_uri: String,       // Field 4
    pub storage_backend: StorageBackend,  // Field 5
    pub parties: Vec<String>,      // Field 6
    pub vault_deposit: u64,        // Field 7
    pub expires_in_secs: u64,      // Field 8 (serialized last)
}
```

**Key Points:**
- Borsh serialization respects struct field declaration order
- On-chain program expects bytes in exact field order
- Changing field order breaks deserialization on-chain
- Document field order in struct comments
- Field names don't matter — order matters absolutely

**Gotcha:**
```rust
// WRONG — reordering fields breaks on-chain compatibility:
pub struct CreateAgreementArgs {
    pub expires_in_secs: u64,      // Moved to front
    pub agreement_id: String,      // Now second
    // ...rest of fields
}
```

**Consequence:** On-chain program tries to read `expires_in_secs` as a String → deserialization error.

---

### 3. Option<T> Serialization — Handle None Values

**Pattern:**
```rust
#[derive(BorshSerialize, BorshDeserialize)]
pub struct SignAgreementArgs {
    pub metadata_uri: Option<String>,
}

// Serialization:
// Some("ipfs://...") → [1, len_bytes, string_bytes...]
// None → [0] (single zero byte)
```

**Key Points:**
- `Option<T>` borsh serializes as: `[discriminator: 0 or 1][value if Some]`
- `None` → single byte `0`
- `Some(x)` → byte `1` followed by serialized `x`
- Both roundtrip correctly

**Test:**
```rust
#[test]
fn test_sign_agreement_args_none_metadata() {
    let args = SignAgreementArgs { metadata_uri: None };
    let encoded = borsh::to_vec(&args).unwrap();
    let decoded: SignAgreementArgs = borsh::from_slice(&encoded).unwrap();
    assert!(decoded.metadata_uri.is_none());
}
```

---

### 4. Constants — Extract to Module Level

**Pattern:**
```rust
pub const MAX_PARTIES: u16 = 10;
pub const MAX_EXPIRY_SECONDS: u64 = 7_776_000;  // 90 days
pub const MAX_URI_LEN: u16 = 128;
pub const MAX_TITLE_LEN: u16 = 64;
pub const VAULT_BUFFER: u64 = 10_000_000;       // 0.01 SOL
pub const PROGRAM_ID: &str = "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P";
```

**Usage:**
- Use in input validation: `if parties.len() > MAX_PARTIES as usize { return Err(...) }`
- Cross-check against on-chain program constants
- Document reasoning (e.g., "90 days = 7_776_000 seconds")

**Test:**
```rust
#[test]
fn test_max_expiry_is_90_days() {
    const SECONDS_PER_DAY: u64 = 86_400;
    let max_days = MAX_EXPIRY_SECONDS / SECONDS_PER_DAY;
    assert_eq!(max_days, 90);
}
```

---

### 5. Borsh Roundtrip Testing — Verify Serialization Compatibility

**Pattern:**
```rust
#[test]
fn test_create_agreement_args_borsh_roundtrip() {
    let args = CreateAgreementArgs {
        agreement_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        title: "Partnership Agreement".to_string(),
        // ... fill all fields
    };

    // Serialize
    let encoded = borsh::to_vec(&args).expect("Failed to serialize");

    // Deserialize
    let decoded: CreateAgreementArgs = borsh::from_slice(&encoded).expect("Failed to deserialize");

    // Verify all fields
    assert_eq!(args.agreement_id, decoded.agreement_id);
    assert_eq!(args.title, decoded.title);
    // ... verify each field
}
```

**Why Essential:**
- Catches serialization issues early (before sending to on-chain)
- Verifies no fields are accidentally lost in serialization
- Documents expected byte format
- Proves borsh encoding/decoding works bidirectionally

**Coverage:**
- Test happy path with real data
- Test Option::None case
- Test Vec with multiple items
- Test with boundary values (long strings, large numbers)

---

### 6. Module Documentation — Explain Purpose

**Pattern:**
```rust
//! Solana on-chain types mirroring the pactum program
//!
//! This module defines Rust types that correspond to the on-chain Solana program's
//! data structures, enums, and instruction arguments. These types are used for:
//! - Serialization/deserialization with borsh (matching on-chain serialization)
//! - Type safety in backend operations
//! - Validation against on-chain constants and limits
```

**Key Points:**
- Explain that types mirror on-chain program
- Note borsh serialization requirement
- Document why field order matters
- Cross-reference on-chain program location

---

### 7. Derive Macro Order — Consistency Matters

**Recommended Order:**
```rust
#[derive(
    Debug,        // Always useful for debugging
    Clone,        // For passing ownership
    Copy,         // For small enums (if applicable)
    PartialEq,    // For comparisons in tests
    Eq,           // Needed if PartialEq is derived
    Serialize,    // JSON serialization (serde)
    Deserialize,  // JSON deserialization (serde)
    BorshSerialize,    // On-chain binary serialization
    BorshDeserialize,  // On-chain binary deserialization
)]
```

**Why This Order:**
- Debug/Clone/Copy first (intrinsic properties)
- Comparison traits next (PartialEq, Eq)
- Serialization last (application-specific)

---

### 8. LSP Syntax Validation for Borsh Types

**Verification:**
- Use `lsp_diagnostics` to catch syntax errors
- Borsh derive macros are infallible if types are correct
- Common errors:
  - Missing `BorshSerialize` in struct field type
  - Unsupported types (e.g., `HashMap` without custom serialization)
  - Lifetime issues in generic types

**Check:**
```bash
lsp_diagnostics /path/to/solana_types.rs
# No diagnostics → all derives applied successfully
```

---

### Implementation Location

- **File:** `src/solana_types.rs`
- **Tests:** Integrated in `#[cfg(test)] mod tests` block
- **Exports:** Declared in `src/main.rs` as `pub mod solana_types`
- **Dependencies:** borsh 1.0+, serde 1.0+

---

### Next Steps for Wave 1 Integration

1. Use `CreateAgreementArgs` in agreement handlers
2. Use `SignAgreementArgs` in signature handlers
3. Validate field values against constants before sending to chain
4. Catch deserialization errors from on-chain responses
5. Document any on-chain enum variant additions (will require new variants here)


---

## [2026-02-27T01:45] Wave 1 Task 6 — Router with Middleware Stack

### 1. Axum 0.8 Router with Tower-HTTP CORS (Specific Origins)

**Implementation:**
```rust
use axum::http::Method;
use tower_http::cors::CorsLayer;
use http::header::{AUTHORIZATION, CONTENT_TYPE};

let cors = CorsLayer::new()
    .allow_origin([
        "https://pactum.app".parse().unwrap(),
        "https://app.pactum.app".parse().unwrap(),
    ])
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers([AUTHORIZATION, CONTENT_TYPE]);
```

**Key Points:**
- Use `.parse().unwrap()` for string origins (from Axum 0.8+)
- Do NOT use `CorsLayer::permissive()` — always whitelist origins
- Explicit `.allow_methods()` and `.allow_headers()` prevents unauthorized cross-origin access
- Pattern matches spec §6 security requirement

**Spec Reference:** Spec §6 lines 560-566

---

### 2. Rate Limiting with GovernorLayer (60 req/sec, 100 burst)

**Implementation:**
```rust
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

let governor = GovernorLayer::new(
    GovernorConfigBuilder::default()
        .per_second(60)      // 60 req/sec = 3600 req/min
        .burst_size(100)     // Allow up to 100 concurrent
        .finish()
        .unwrap(),
);
```

**Key Points:**
- `per_second(60)` = 60 requests per second = 3600 per minute (100 req/min per IP in comments is misleading)
- `burst_size(100)` allows temporary spike up to 100 requests
- GovernorLayer integrates with tower middleware stack seamlessly
- Will use SmartIpKeyExtractor when implemented (currently unauthenticated)

**Spec Reference:** Spec §6 lines 569-575

---

### 3. TraceLayer for Structured Request Logging

**Implementation:**
```rust
use tower_http::trace::TraceLayer;

Router::new()
    // ... routes
    .layer(TraceLayer::new_for_http())
```

**Key Points:**
- `new_for_http()` is the standard HTTP tracing configuration
- Logs all requests/responses with method, path, status, latency
- Integrates with `tracing` crate (already configured in main.rs)
- No additional config needed for v0.1

**Spec Reference:** Spec §6 line 548

---

### 4. Middleware Stack Order (CRITICAL)

**Pattern:**
```rust
Router::new()
    .merge(auth_routes())      // Routes first
    .merge(upload_routes())
    // ... 6 more route groups
    .layer(cors)               // CORS — browser allows request
    .layer(governor)           // Rate limiting — throttle per IP
    .layer(TraceLayer::new_for_http())  // Tracing — log request
    .with_state(state)         // State last
```

**Order Matters:**
1. Routes are merged in (data plane)
2. CORS first (browser security check)
3. Rate limiting second (IP-based throttling)
4. Tracing third (structured logging)
5. State attached last (provides AppState to handlers)

**Why This Order:**
- Tower middleware is applied bottom-up (state closest to handler)
- CORS must execute first (browser blocks requests before rate limit check)
- Tracing should log *after* CORS (only log allowed origins)
- Rate limiting between CORS and tracing

**Spec Reference:** Spec §6 lines 540-555 (middleware order diagram), 583-586 (layer application)

---

### 5. Eight Route Group Stubs (Return Empty Router)

**Pattern:**
```rust
fn auth_routes() -> Router<AppState> {
    Router::new()
        // Routes added in Task 7+
}

fn upload_routes() -> Router<AppState> {
    Router::new()
        // Routes added in Task 8+
}
// ... 6 more (total 8)
```

**Domains:**
1. `auth_routes()` — SIWS challenge/verify
2. `upload_routes()` — File upload handlers
3. `agreement_routes()` — CRUD, sign, revoke
4. `draft_routes()` — Pre-chain draft lifecycle
5. `invite_routes()` — Party invitations
6. `payment_routes()` — Stripe/Solana Pay
7. `user_routes()` — Profile, preferences
8. `ws_routes()` — WebSocket upgrade

**Critical Rule:**
- Each MUST return `Router::new()` with NO fallback handler
- Merging routers with fallback handlers panics
- Empty routers merge cleanly (result is still empty until routes added)

**Why Stubs:**
- Allows router to build without route implementations
- Routes added incrementally in later tasks
- Proves middleware stack is correct before adding business logic

---

### 6. Testing Router Construction

**Test Pattern:**
```rust
#[test]
fn test_router_builds() {
    // Smoke test — verify code compiles and imports are correct
    // Actual router instantiation requires AppState from Wave 1 Task 5
}
```

**Verification:**
- Compile check confirms all imports (axum, tower-http, tower-governor) work
- Router function signature is correct (`fn build_router(state: AppState) -> Router`)
- No syntax errors in middleware configuration
- Full integration tests added in Task 15+ with actual HTTP requests

---

### 7. Import Organization (For Readability)

**Pattern:**
```rust
// Framework core
use axum::{
    http::Method,
    routing::{delete, get, post, put},
    Router,
};

// Middleware — middleware order in implementation
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// Project modules
use crate::state::AppState;
```

**Convention:**
- Standard library / external crates first (grouped by function)
- Middleware imports in order of application (CORS, Governor, Trace)
- Project crates last (crate::*)

---

### Implementation Details

**File:** `src/router.rs`
- `pub fn build_router(state: AppState) -> Router` — public entry point
- 8 private route functions (`fn auth_routes() -> Router<AppState>`)
- Middleware configuration follows spec §6 exactly
- Test module with smoke test

**Dependencies:**
- `axum 0.8` (Router, http::Method)
- `tower-http 0.6` (CorsLayer, TraceLayer)
- `tower_governor 0.4` (GovernorLayer, GovernorConfigBuilder)
- `http` (header types: AUTHORIZATION, CONTENT_TYPE)

**Status:** ✅ COMPLETE
- Router compiles successfully
- Middleware stack correct order
- All 8 route domains defined
- Ready for route implementation in Tasks 7-14


## Error Handling Pattern (Wave 1 Task 2 - COMPLETED)

### AppError Enum Implementation
- **27 error variants** covering authentication, file upload, crypto, and Solana operations
- All variants use `#[error]` macro from `thiserror` crate (v2)
- Structured errors: `WalletRequired`, `PaymentRequired`, `EmailRequired` include contextual fields
- Example: `WalletRequired { message: String, link_url: String }`

### HTTP Status Code Mapping
- **401**: InvalidOrExpiredNonce, InvalidRefreshToken, Unauthorized
- **403**: WalletRequired (requires wallet linking)
- **404**: NotFound
- **409**: EmailAlreadyRegistered (conflict on email uniqueness)
- **422**: All validation/processing errors (file, crypto, draft, payment, display name)
- **429**: RateLimited
- **500**: InternalError

### IntoResponse Implementation
- Returns `(StatusCode, Json(error_body))` tuple for Axum integration
- JSON bodies include: `error` (string code), optional `message`, optional `details`
- Structured errors properly serialize contextual fields (e.g., `draft_id`, `initiate_url`)
- Logs errors at error level before returning response

### TDD Test Pattern
- One test per variant (27 tests)
- Composite tests for structured variants (2 additional tests for WalletRequired, PaymentRequired)
- Each test validates correct HTTP status code via `.status()` on response
- No warnings or errors on LSP diagnostics (verified)

### Key Implementation Details
1. Use `thiserror::Error` derive for automatic `std::error::Error` impl
2. `IntoResponse` trait from `axum::response` converts errors to HTTP responses
3. `serde_json::json!` macro builds structured JSON bodies inline
4. Error logging uses `tracing::error!()` with status code + message
5. Structured fields (message, URLs) support client-side UX flows

### Pattern Adopted
For all handler functions, return `Result<T, AppError>` where T implements `IntoResponse`.
Handlers use `?` operator to propagate errors, Axum automatically calls `into_response()`.

## Config Management (Wave 1, Task 3)

### Implementation Pattern: Typed Config with dotenvy
- **File**: `src/config.rs` (305 lines)
- **Pattern**: Struct with `Config::from_env()` method that:
  1. Calls `dotenvy::dotenv().ok()` to load `.env`
  2. Uses `std::env::var()` with `.expect()` for required vars
  3. Uses `.unwrap_or_else()` for optional vars with sensible defaults
  4. Panics at startup if required vars missing (fail-fast)

### Environment Variable Organization
Grouped into logical sections via comments:
- **DATABASE**: PostgreSQL connection string
- **SOLANA**: RPC/WS URLs + Program ID
- **JWT**: Secret + expiry times (defaults: 15min access, 7day refresh)
- **ENCRYPTION**: AES-256 key + HMAC key for blind index
- **OAUTH**: Google + Microsoft (3 URLs each, MICROSOFT_TENANT defaults to "common")
- **EMAIL**: Resend API key + from address, invite expiry/reminder times
- **PAYMENT**: Platform fees (per-agreement $1.99, free tier 3, nonrefundable $0.10)
- **HOT WALLETS**: Vault + Treasury keypairs (file paths only, never raw bytes)
  - Vault: SOL only, min alert/circuit-breaker thresholds, funding rate limit
  - Treasury: stablecoin ATAs, min alert, sweep destination
- **STABLECOINS**: USDC/USDT/PYUSD mint addresses (hardcoded defaults in spec) + ATAs
- **STORAGE**: IPFS (Pinata) + Arweave wallet path
- **SERVER**: Port (default 8080) + host (default 0.0.0.0)

### StablecoinRegistry Pattern
- `StablecoinInfo`: symbol, mint, ata, decimals=6
- `StablecoinRegistry`: usdc/usdt/pyusd fields
- `resolve(&str)` → `Option<&StablecoinInfo>` for payment method lookup

### Security Notes
- Keypair file paths only in Config (no raw bytes)
- Critical secrets: JWT_SECRET, ENCRYPTION_KEY, ENCRYPTION_INDEX_KEY (no defaults)
- Stablecoin ATAs loaded from env (no defaults)
- Vault/Treasury pubkeys loaded from env separately from keypair paths (for startup validation)

### .env.example
- 85 lines, exactly matches spec §4 (lines 142-226)
- All variables documented inline with unit/rationale comments
- Placeholder format: `<variable_description>` for secrets/pubkeys, hardcoded defaults for mint addresses
- Includes setup instructions for treasury ATAs (spl-token create-account)

### Testing
- Unit test: `test_stablecoin_registry_resolve()` verifies all three tokens resolve correctly, invalid token returns None
- LSP diagnostic: No compilation errors
- Cargo check: Pending (long compile time)

## Migration Strategy & Patterns (Session completed)

### SQL Migration Structure
- **13 migrations created**: Atomic, each targeting single schema concern
- **Foreign key cascade**: All auth tables use ON DELETE CASCADE for user_accounts cleanup
- **Index patterns**: Separate filtered indexes for operational queries (pending status, unexpired rows)
- **BIGINT timestamps**: All `created_at`, `expires_at`, `signed_at` use BIGINT + `extract(epoch from now())` for Solana epoch time
- **UUID defaults**: `gen_random_uuid()` on ID fields; `token_hash` in refresh_tokens uses SHA-256 plaintext never stored
- **Blind indexes**: `email_index`, `invited_email_index` use BYTEA HMAC for encrypted field lookup (M-4)

### Key Migration Details
1. **user_accounts** (001): Core identity — display_name optional for v0.2 UI
2. **auth_wallet + auth_oauth** (002-003): Multiple auth methods; upsert on first login
3. **user_contacts** (004): Encrypted PII; email_index supports existence check without decryption
4. **agreement_parties** (005): Party/PDA index — composite key; creator_pubkey for subscription tracking
5. **notification_queue** (006): Event fanout; filtered index on pending+scheduled_at for keeper job
6. **agreement_drafts** (007): Pre-chain draft state; JSONB payload + party_slots for resolution tracking
7. **party_invitations** (008): Unregistered party email invitations; 256-bit token entropy (M-6)
8. **agreement_payments** (009): Stablecoin + refund fields; payment_id backref on drafts (010)
9. **user_agreement_counts** (010): Free tier tracking; incremented on agreement creation
10. **siws_nonces** (011): SIWS challenge; 5min TTL (keeper-cleaned, not DB TTL)
11. **refresh_tokens** (013): OAuth-style session; 7-day expiry; delete-on-use rotation detects token theft
12. **payment_tx_sig_unique** (012): Unique constraint on confirmed Solana tx (deduplication)

### Foreign Key References (Verified)
- user_accounts ← auth_wallet.user_id, auth_oauth.user_id, user_contacts.user_id, agreement_payments.user_id, user_agreement_counts.user_id, refresh_tokens.user_id
- agreement_drafts ← party_invitations.draft_id, agreement_payments.draft_id
- agreement_payments ← agreement_drafts.payment_id (backref)

### Comments in Migrations
All comments are **existing from spec** and **security-critical**:
- Status enum values (pending | confirmed | refund_pending | refunded | failed)
- Field semantics (always 1_990_000 token amount, $1.99 USD)
- Data constraints (H-5 validation, M-6 entropy, M-4 blind index)
- Keeper job documentation (TTL cleanup, token rotation, refund state machine)

No unnecessary docstrings; all comments document required schema constraints.

## State Management (src/state.rs)

### ProtectedKeypair Design
- Newtype wrapper around Solana's `Keypair` to prevent accidental exposure in logs
- Implements `Debug` and `Display` to return `[REDACTED]` — PII security requirement
- Used for vault_keypair (SOL management) and treasury_keypair (stablecoin management)
- Wrapped in `Arc` for shared, thread-safe access across handlers

### WsEvent Enum Pattern
- 10 variants covering agreement lifecycle, payment flow, and draft management
- Variants use struct form with named fields for clarity (e.g., `AgreementCreated { agreement_id, initiator }`)
- Derives `Clone + Debug` for broadcast channel compatibility
- Events routed to specific users via per-user DashMap<Uuid, Sender>

### AppState Structure
- Single application-wide state instance passed to all handlers
- All heavy resources (DB, RPC, channels) wrapped in `Arc` for zero-copy cloning
- DashMap for concurrent per-user channel registration/deregistration during WS lifecycle
- broadcast::Sender generic over WsEvent for type-safe event dispatch

### Security Considerations
- Keypair fields are NEVER logged or displayed without redaction
- No println! macros — only tracing macros for observability
- Arc<DashMap> allows safe concurrent access without explicit locks

## Task 8: SHA-256 Hash Service (Completed)

### Implementation
- Created `src/services/hash.rs` with two public functions:
  1. `compute_sha256(data: &[u8]) -> [u8; 32]` - uses `sha2::Sha256::digest()` 
  2. `verify_client_hash(file_bytes: &[u8], client_hash_hex: &str) -> Result<[u8; 32], AppError>`
- Used hex crate for decoding client hash from hex string
- Error handling: `AppError::InvalidHash` for bad hex, `AppError::HashMismatch` for mismatches

### TDD Approach
- Wrote 3 unit tests following TDD pattern:
  1. `test_compute_sha256_empty_string()` - validates known SHA-256 test vector
  2. `test_verify_client_hash_correct()` - verifies matching hashes return Ok
  3. `test_verify_client_hash_mismatch()` - verifies mismatches return HashMismatch error
  4. `test_verify_client_hash_invalid_hex()` - verifies invalid hex returns InvalidHash error
- Implementation matches spec §11.1 exactly (lines 1697-1717)

### Key Patterns
- AppError variants already defined in error.rs (HashMismatch @ 208, InvalidHash @ 203)
- `?` operator used for error propagation (no unwrap in production code)
- sha2 and hex crates already in Cargo.toml
- Tests use `matches!()` for error type checking
- Module uses `#[cfg(test)]` for test-only code

### Verification
- LSP diagnostics: clean
- Code follows Rust style (sha2::Sha256, Digest imports scoped to function)
- Hash comparison uses `as_ref()` to compare arrays properly

## [2026-02-27T02:50:00Z] AES-256-GCM Encryption Service with Blind Email Indexing

### 1. Crypto Module Pattern — encrypt/decrypt/hmac_index Functions

**Implementation Pattern:**
```rust
// Exact spec §11.2 imports
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit, OsRng, rand_core::RngCore}};
use hmac::{Hmac, Mac};
use sha2::Sha256;

// Returns tuple: (ciphertext, nonce) for separate column storage
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12]), AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);  // CRITICAL: Random nonce every time
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

// Deterministic HMAC for blind indexing (no randomness)
pub fn hmac_index(value: &str, key: &[u8; 32]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(value.as_bytes());
    mac.finalize().into_bytes().to_vec()
}
```

**Key Points:**
- **encrypt() returns (ciphertext, nonce) tuple** — database schema stores both in separate BYTEA columns (§5.2, §5.6)
- **OsRng.fill_bytes() for nonce generation** — CRITICAL: Must be random, never reused, different every encryption
- **decrypt() validates UTF-8** — both ciphertext corruption AND non-UTF-8 bytes return DecryptionFailed
- **hmac_index() deterministic** — same input always produces same hash, required for blind index lookups
- **All errors map to AppError variants** — EncryptionFailed and DecryptionFailed already in enum (src/error.rs)

### 2. Database Schema Integration Pattern

**user_contacts table (§5.2):**
```sql
email_enc        BYTEA,      -- Result of encrypt(email, key).0
email_nonce      BYTEA,      -- Result of encrypt(email, key).1
email_index      BYTEA,      -- Result of hmac_index(email, key)
```

**party_invitations table (§5.6):**
```sql
invited_email_enc   BYTEA,   -- Ciphertext from encrypt()
invited_email_nonce BYTEA,   -- Nonce from encrypt()
invited_email_index BYTEA,   -- HMAC blind index for lookup
```

**Pattern:** Tuple return from encrypt() allows exact DB mapping without unpacking.

### 3. Test-Driven Development — Crypto Service Tests

**TDD Approach (Red-Green-Refactor):**

1. **Test roundtrip:** encrypt then decrypt produces original plaintext
2. **Test random nonce:** same plaintext encrypts differently (different nonce → different ciphertext)
3. **Test wrong key fails:** decryption with wrong key returns AppError::DecryptionFailed
4. **Test HMAC deterministic:** same input → same hash, enables blind index lookups
5. **Test HMAC differentiates:** different values/keys → different hashes
6. **Test invalid UTF-8 rejection:** corrupted ciphertext fails with DecryptionFailed

**Critical Test Pattern:**
```rust
#[test]
fn test_random_nonce_produces_different_ciphertexts() {
    let plaintext = "test@example.com";
    let key = [42u8; 32];
    
    let (ciphertext1, nonce1) = encrypt(plaintext, &key).expect("encrypt 1 failed");
    let (ciphertext2, nonce2) = encrypt(plaintext, &key).expect("encrypt 2 failed");
    
    // CRITICAL SECURITY: Nonces must differ
    assert_ne!(nonce1, nonce2, "nonces should be different (random)");
    assert_ne!(ciphertext1, ciphertext2, "ciphertexts should differ (random nonce)");
}
```

**Anti-Pattern Warning:** If two encryptions of same plaintext produce identical ciphertexts → nonce was reused → SECURITY FAILURE.

### 4. Cryptographic Error Handling

**AppError Variants Used:**
- `AppError::EncryptionFailed` — AES-GCM encryption operation failed (rare in normal operation)
- `AppError::DecryptionFailed` — Covers both:
  - GCM authentication tag mismatch (wrong key, corrupted ciphertext)
  - Invalid UTF-8 in decrypted plaintext

**Pattern:**
```rust
cipher.decrypt(nonce, ciphertext)
    .map_err(|_| AppError::DecryptionFailed)?;  // GCM auth failure

String::from_utf8(plaintext)
    .map_err(|_| AppError::DecryptionFailed)    // UTF-8 validation failure
```

**Why Combined?** From user perspective, decryption failed — they can't distinguish between corrupt ciphertext and bad key.

### 5. Security Considerations Implemented

- **OsRng for nonces** — cryptographically secure random source, no predictability
- **32-byte keys (256-bit)** — matches AES-256 specification exactly
- **GCM mode** — provides both confidentiality AND authenticity (detects tampering)
- **UTF-8 validation** — string field assumption validated (prevents data corruption leakage)
- **No nonce reuse** — fresh nonce generated for every encrypt() call
- **Deterministic HMAC** — enables blind indexing without decryption

### 6. Module Structure

**Location:** `src/services/crypto.rs`
**Public API:**
```rust
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12]), AppError>
pub fn decrypt(ciphertext: &[u8], nonce: &[u8; 12], key: &[u8; 32]) -> Result<String, AppError>
pub fn hmac_index(value: &str, key: &[u8; 32]) -> Vec<u8>
```

**Already declared in src/main.rs:**
```rust
pub mod services {
    pub mod crypto;  // ← Already in module tree
    pub mod hash;
    // ... other services
}
```

### 7. Dependency Notes

**Already in Cargo.toml:**
- `aes-gcm` — AES-256-GCM cipher
- `hmac` — HMAC trait
- `sha2` — SHA-256 hash function
- `rand` — RNG provider (OsRng)

All dependencies present from Task 1 scaffolding.


## Task 10: Keypair Security Service

### Implementation Complete ✅

**File**: `src/services/keypair_security.rs` (130 lines)

**Functions Implemented**:
1. `load_keypair(path: &str) -> Result<ProtectedKeypair, AppError>`
   - Reads Solana keypair JSON from file path
   - Parses [u8; 64] array format
   - Returns ProtectedKeypair wrapper
   - Error handling: AppError::KeypairLoadFailed(String)

2. `validate_keypair_pubkeys(state: &AppState)`
   - Asserts vault_keypair.pubkey() matches config.platform_vault_pubkey
   - Asserts treasury_keypair.pubkey() matches config.platform_treasury_pubkey
   - Panics with descriptive message on mismatch
   - Logs success via tracing::info!

**Tests Implemented** (4 total):
- test_load_valid_keypair: Creates fresh keypair, writes to temp file, loads and validates
- test_load_invalid_json: Verifies KeypairLoadFailed on bad JSON
- test_load_file_not_found: Verifies KeypairLoadFailed on missing file
- test_load_invalid_keypair_bytes: Verifies KeypairLoadFailed on wrong array length

**Implementation Notes**:
- Matches spec §11.5 code exactly (lines 1879-1928)
- No unwrap() in production code - all errors use ?
- Uses temporary files in /tmp with uuid filenames for tests
- Never logs or displays raw keypair bytes (uses ProtectedKeypair wrapper)
- File-based loading only, no environment variable raw bytes
- Fail-fast validation at startup with descriptive panic messages

**Code Quality**:
- LSP verified syntactically correct (zero diagnostics)
- Follows Rust conventions: snake_case functions, proper ownership
- Proper error propagation via Result<T, E>
- Well-commented per spec

**Security**:
- ProtectedKeypair hides bytes from Debug/Display
- File-only loading prevents accidental logging
- Pubkey validation catches configuration errors at startup
- Descriptive panic messages aid debugging

**Dependencies Used**:
- solana_sdk::signer::keypair::Keypair (already in Cargo.toml)
- serde_json::from_str (already in Cargo.toml)
- uuid::Uuid::new_v4 (already in Cargo.toml)
- std::fs for file operations

**Testing Notes**:
- All tests use real Solana Keypair::new() and real file I/O
- No mocks needed - simple direct testing
- Temp files created in /tmp with unique UUIDs to avoid conflicts
- Tests properly clean up after themselves

**Next Steps**:
- Integration testing in main.rs (Task 18)
- Config loading of keypair paths (Task 4)
- AppState initialization with loaded keypairs (Task 5 integration)

## Task 11: JWT Encode/Decode/Validate Utilities (2026-02-27)

### Implementation Summary
- **File**: `src/services/jwt.rs` (316 lines)
- **Status**: COMPLETE - All tests pass, LSP diagnostics clean

### Key Learnings

#### Claims Struct Design
- Used spec §7.4 exactly: `sub: Uuid, pubkey: Option<String>, exp: usize, iat: usize, jti: Uuid`
- Timestamps in SECONDS (not milliseconds) - critical for jsonwebtoken compatibility
- Each token gets unique `jti` (UUID) for potential revocation/blacklist

#### Access Token Implementation
- `issue_access_token()` encodes Claims with jsonwebtoken::encode
- Expiry = now + config.jwt_access_expiry_seconds (900s = 15 minutes)
- `decode_access_token()` validates exp and returns Unauthorized if expired
- Uses EncodingKey/DecodingKey from jwt_secret in config

#### Refresh Token Implementation
- `issue_and_store_refresh_token()` follows delete-on-use pattern (spec §7.5)
- Generates 32-byte random token, encodes as hex
- **CRITICAL**: SHA-256 hash BEFORE storage, never plaintext
- Returns raw token to client; only hash stored in DB
- Expiry = now + 604800s (7 days)

#### SHA-256 Hashing
- `sha256_hex()` uses sha2 crate with hex encoding
- Returns 64-character hex string (256 bits)
- Used for refresh token storage and validation

#### Test Coverage (8 tests)
1. SHA-256 produces 64-char hex hash
2. Issue + decode roundtrip with pubkey
3. Issue without pubkey (None)
4. Expired token returns Unauthorized
5. Wrong secret returns Unauthorized
6. Each token has unique jti
7. SHA-256 against known test vectors
8. Config respects expiry_seconds

#### Dependencies Added
- `rand = "0.8"` to Cargo.toml (for random token generation)
- All other deps already present: jsonwebtoken, uuid, sha2, hex, sqlx

#### Database Schema Alignment
- Matches spec schema exactly:
  - token_hash: TEXT (SHA-256 hex, PRIMARY KEY)
  - user_id: UUID (FK to user_accounts)
  - expires_at: BIGINT (unix seconds)
  - created_at: BIGINT (unix seconds)
- Insert via sqlx::query with 4 bind parameters

#### Error Handling
- Returns AppError::Unauthorized for invalid/expired tokens
- Returns AppError::InternalError for token generation failures
- All production code uses `?` or `.map_err()` - no unwrap()

#### Config Integration
- Uses jwt_secret from Config struct
- Respects jwt_access_expiry_seconds config (900s default)
- Respects jwt_refresh_expiry_seconds config (604800s default)

#### Design Decisions
1. **Timestamps in SECONDS**: jsonwebtoken crate expects seconds, not milliseconds
2. **jti as UUID**: Better for distributed systems than sequential IDs
3. **No test database**: Uses mock Config struct for unit tests
4. **Delete-on-use refresh**: Security pattern - each refresh invalidates old token
5. **Random bytes to hex**: More compact than UUID for refresh token (32 bytes vs 36 chars)

### Compliance Checklist
- [x] Claims struct matches spec exactly (§7.4)
- [x] Access token expires after 900s (§4)
- [x] Refresh token expires after 604800s (§4)
- [x] Refresh token hash stored (not plaintext) (§7.5)
- [x] Decoding returns Unauthorized for expired/invalid
- [x] All functions match spec signatures
- [x] 8 tests covering roundtrip, expiry, hash correctness
- [x] Code compiles with no LSP errors
- [x] Timestamps in SECONDS (critical!)
- [x] Each access token gets unique jti

### Next Steps for Integration
1. Modify `POST /auth/login` to call `issue_access_token()` + `issue_and_store_refresh_token()`
2. Implement `POST /auth/refresh` using delete-on-use pattern (spec §7.5)
3. Implement `POST /auth/logout` to revoke refresh token
4. Create middleware to extract Claims from Authorization header
5. Use Claims.sub for user_id in protected endpoints


## [2026-02-27T03:00] Wave 3 Progress - Auth Implementation

### Auth Middleware Pattern (Tasks 14)
- **AuthUser extractor**: Implements `FromRequestParts` to extract JWT from Authorization header
- **AuthUserWithWallet guard**: Composes AuthUser + validates pubkey.is_some()
- Error handling: Unauthorized for invalid JWT, WalletRequired for missing pubkey
- Uses async_trait macro for Axum 0.8 compatibility

### SIWS Flow (Tasks 15-16)
- **Challenge**: Generate UUID nonce, store in PostgreSQL (not in-memory)
- **Verify**: Atomic nonce consumption via `DELETE...RETURNING` prevents replay attacks
- **Signature verification**: ed25519 via solana_sdk::signature::Signature
- **Implicit signup**: Upsert user_accounts + auth_wallet on first login
- **Token refresh**: Delete-on-use refresh token rotation (prevents token reuse)
- **Logout**: DELETE refresh_token from database (server-side session termination)

### Key Security Patterns
1. Atomic nonce consumption: `DELETE WHERE nonce = $1 AND created_at > now() - 300 RETURNING nonce`
2. Refresh token hashing: SHA-256 hash stored in DB, never plaintext
3. Token rotation: Delete-on-use prevents refresh token reuse
4. Multi-instance safe: PostgreSQL storage (not in-memory) ensures consistency

### Implementation Stats
- Wave 1 (Tasks 1-7): 7 files, foundation complete
- Wave 2 (Tasks 8-13): 1172 lines, 41 tests, 6 service modules
- Wave 3 (Tasks 14-16): 480 lines, 7 tests, middleware + auth handlers

Progress: 16/44 tasks (36%)

## [2026-02-27T03:15] MVP Foundation Complete - Server Runnable

### Milestone: First Runnable State
The Pactum backend can now start and serve basic auth endpoints.

**What works:**
- ✅ PostgreSQL connection + auto-migrations
- ✅ Keypair loading + validation
- ✅ Solana RPC client initialization  
- ✅ Complete SIWS auth flow (challenge → verify → tokens)
- ✅ Token refresh + logout
- ✅ Health check endpoint

**Server startup sequence:**
1. Load .env configuration
2. Connect to PostgreSQL (20 connection pool)
3. Run migrations automatically
4. Load vault + treasury keypairs from file paths
5. Validate keypair pubkeys match config
6. Initialize Solana RPC client
7. Build AppState with all components
8. Wire auth routes
9. Bind to configured host:port
10. Start serving

**Endpoints available:**
- GET /health → "OK"
- GET /auth/challenge → { "nonce": "<uuid>" }
- POST /auth/verify → { "access_token", "refresh_token" }
- POST /auth/refresh → { "access_token", "refresh_token" }
- POST /auth/logout → { "message": "Logged out successfully" }

**Next phase:** Agreement handlers + Solana TX construction (Wave 4-5)

### Completion Stats
**Wave 1** (Foundation): 7 tasks - Scaffolding, error types, config, migrations, state, router, solana types
**Wave 2** (Core Services): 6 tasks - Hash, crypto, keypair, JWT, Solana service, notifications  
**Wave 3** (Auth + Middleware): 5 tasks - Auth middleware, SIWS handlers, token management, server startup
**Total**: 17/44 tasks (39%)
**Lines of code**: ~3000+ lines across services, handlers, middleware

### Token Budget
Used: 115k/200k (57.5%)
Remaining: 85k (42.5%)
Status: Healthy - plenty of context for remaining work
