# Pactum Backend — Work Plan

## TL;DR

> **Quick Summary**: Implement the full Pactum backend (Rust/Axum/PostgreSQL/Solana) per `pactum_backend_spec.md` v0.1.0. MVP vertical slice first (auth → create agreement → sign → complete), then expand to drafts, invitations, payments, and background workers.
> 
> **Deliverables**:
> - Complete Rust backend binary (`pactum-backend`) with Axum HTTP + WebSocket server
> - 13+ PostgreSQL migrations
> - 8 handler modules, 8 service modules, 4 background workers
> - Docker Compose + Dockerfile for deployment
> - TDD test suite (unit + integration)
> 
> **Estimated Effort**: XL
> **Parallel Execution**: YES — 9 waves
> **Critical Path**: Scaffolding → Config/Error/State → JWT Auth → Solana TX Builder → Agreement Handlers → Workers → Docker

---

## Context

### Original Request
Implement the Pactum backend per `pactum_backend_spec.md` — a Rust/Axum server that acts as a UX convenience layer for the Pactum on-chain Solana program (Anchor 0.32.1 at `DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P`). The backend never holds signing authority; it constructs partially-signed transactions and returns them to the client.

### Interview Summary
**Key Discussions**:
- **Solana env**: Devnet for development and testing
- **Build priority**: MVP vertical slice — SIWS auth → create agreement → sign → complete, then expand
- **TX construction**: Raw solana-sdk 2.2.x with manual Anchor discriminators (SHA256("global:<name>")[0..8] + borsh args)
- **Test strategy**: TDD with unit tests (mocked Solana RPC, email) + integration tests (real PostgreSQL via docker-compose)
- **SDD**: Spec = Requirements + Design already complete; this plan is Phase 3 (Task Planning)

**Research Findings**:
- On-chain program: 8 instructions (create/sign/cancel/expire/vote_revoke/retract/close + initialize_collection)
- PDA seeds: agreement = `[b"agreement", creator, agreement_id]`, mint_vault = `[b"mint_vault", agreement_key]`, pda_authority = `[b"mint_authority", b"v1"]`
- Axum 0.8: `/{param}` syntax, no `#[async_trait]`, WebSocket via `WebSocketUpgrade`
- SQLx 0.8: compile-time checked queries, `tls-rustls-ring-webpki` feature
- AES-GCM: random nonces mandatory, 12-byte nonce stored alongside ciphertext

### Gap Analysis (self-conducted — Metis timed out)
**Identified Gaps** (addressed in plan):
- SQLx compile-time query checking requires `DATABASE_URL` at build time → plan includes sqlx offline mode setup
- OAuth2 state parameter for CSRF protection not detailed in spec → task includes CSRF state handling
- Error enum design needs comprehensive variant list → dedicated error.rs task with all variants from spec
- `resend-rs` version in Cargo.toml is 0.8 but spec says "latest" → verified 0.8 exists on crates.io
- MPL-Core program at `CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d` needed for sign_agreement NFT minting → devnet has it per Anchor.toml test.genesis

---

## Work Objectives

### Core Objective
Build the complete Pactum backend binary from scratch — a Rust HTTP + WebSocket server that provides authentication, agreement management, payment processing, and real-time notifications for the Pactum on-chain protocol.

### Concrete Deliverables
- `pactum-backend` binary (Cargo.toml + 35+ source files)
- 13+ PostgreSQL migration files
- Docker Compose + Dockerfile for containerized deployment
- `.env.example` with all ~60 configuration variables
- TDD test suite with unit + integration coverage

### Definition of Done
- [ ] `cargo build --release` succeeds with zero errors
- [ ] `cargo test` passes all unit tests
- [ ] `cargo clippy -- -D warnings` passes
- [ ] Integration tests pass against a real PostgreSQL instance
- [ ] Docker compose starts successfully (api + postgres)
- [ ] MVP flow works: SIWS auth → POST /agreement → POST /agreement/{pda}/sign → GET /agreement/{pda} shows PendingSignatures/Completed

### Must Have
- All API routes from spec §8 implemented
- All 13+ database migrations
- SIWS + OAuth2 (Google, Microsoft) authentication
- JWT access/refresh token architecture
- Solana transaction construction for all 8 on-chain instructions
- AES-256-GCM encryption for PII (user contacts)
- Background workers: event_listener, keeper, refund_worker, notification_worker
- Payment: stablecoin (USDC/USDT/PYUSD) via Solana Pay
- WebSocket per-user event channels

### Must NOT Have (Guardrails)
- **No Stripe integration** — deferred to v0.2
- **No Apple OAuth** — deferred to future version
- **No gasless signing** — deferred to v0.2
- **No client-side document encryption** — deferred to v0.2
- **No MPC wallets** — deferred to v0.3
- **No multi-session WS fan-out** — single active session per user for v0.1
- **No `lazy_static!`** — use `std::sync::LazyLock` (Rust 1.80+)
- **No `unwrap()` in production code** — use `expect()` with context or `?` propagation
- **No raw base58 private keys in env vars** — keypairs loaded from file paths only
- **No signing transactions on behalf of users** — backend constructs, client signs
- **No PII in logs** — ProtectedKeypair redacts, email never logged in plaintext
- **No `anchor-client` dependency** — raw solana-sdk instruction construction only

---

## Verification Strategy

> **ZERO HUMAN INTERVENTION** — ALL verification is agent-executed. No exceptions.

### Test Decision
- **Infrastructure exists**: NO (greenfield)
- **Automated tests**: YES (TDD)
- **Framework**: `cargo test` (Rust built-in) + SQLx test utilities
- **If TDD**: Each task follows RED (failing test) → GREEN (minimal impl) → REFACTOR

### Test Infrastructure Setup (Task 1)
- `#[cfg(test)] mod tests` in each source file for unit tests
- `tests/` directory for integration tests
- `docker-compose.test.yml` for PostgreSQL test instance
- `sqlx prepare` for offline compile-time query checking in CI
- Mock traits for external services (Solana RPC, email, storage)

### QA Policy
Every task MUST include agent-executed QA scenarios.
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **API endpoints**: Use Bash (curl) — Send requests, assert status + response fields
- **Database**: Use Bash (sqlx/psql) — Run queries, verify schema
- **Compilation**: Use Bash (cargo build/test/clippy) — Verify build succeeds
- **Integration**: Use Bash (docker-compose + curl) — End-to-end flow verification

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Foundation — 7 parallel tasks):
├── Task 1: Project scaffolding + Cargo.toml + test infrastructure [quick]
├── Task 2: Error types (error.rs) — full AppError enum [quick]
├── Task 3: Configuration module (config.rs + .env.example) [quick]
├── Task 4: Database migrations (13+ SQL files) [quick]
├── Task 5: AppState + ProtectedKeypair (state.rs) [quick]
├── Task 6: Router skeleton + middleware stack (router.rs) [quick]
└── Task 7: Solana constants + types module [quick]

Wave 2 (Core Services — 6 parallel tasks):
├── Task 8: SHA-256 hash service (services/hash.rs) [quick]
├── Task 9: AES-256-GCM crypto service (services/crypto.rs) [quick]
├── Task 10: Keypair security service (services/keypair_security.rs) [quick]
├── Task 11: JWT encode/decode/validate utilities [quick]
├── Task 12: Solana service foundation: PDA, discriminators, RPC wrapper (services/solana.rs) [deep]
└── Task 13: Notification service skeleton (services/notification.rs) [quick]

Wave 3 (Auth + Middleware MVP — 5 parallel tasks):
├── Task 14: JWT middleware extractor: AuthUser + AuthUserWithWallet (middleware/) [deep]
├── Task 15: SIWS auth: challenge + verify (handlers/auth.rs - SIWS) [deep]
├── Task 16: Token refresh + logout (handlers/auth.rs - refresh) [quick]
├── Task 17: User handlers: profile + contacts (handlers/user.rs) [quick]
└── Task 18: main.rs MVP: wire foundation + auth routes + server startup [quick]

Wave 4 (Agreement MVP — 5 parallel tasks):
├── Task 19: create_agreement TX construction (services/solana.rs - create) [deep]
├── Task 20: sign_agreement TX construction (services/solana.rs - sign) [deep]
├── Task 21: POST /agreement handler (handlers/agreement.rs - create) [deep]
├── Task 22: POST /agreement/{pda}/sign handler (handlers/agreement.rs - sign) [deep]
└── Task 23: GET /agreement/{pda} + GET /agreements (handlers/agreement.rs - read) [unspecified-high]

Wave 5 (Expansion — 6 parallel tasks):
├── Task 24: OAuth2 Google + Microsoft (handlers/auth.rs - OAuth) [deep]
├── Task 25: POST /auth/link/wallet handler [quick]
├── Task 26: Upload handler: multipart + hash verify (handlers/upload.rs) [unspecified-high]
├── Task 27: Storage service: IPFS + Arweave upload (services/storage.rs) [unspecified-high]
├── Task 28: Metadata generation: NFT metadata JSON (services/metadata.rs) [quick]
└── Task 29: WebSocket handler: upgrade + per-user channels (handlers/ws.rs) [unspecified-high]

Wave 6 (Drafts + Invitations + Payment — 7 parallel tasks):
├── Task 30: Draft handlers: GET/DELETE/PUT/POST /draft/* (handlers/draft.rs) [deep]
├── Task 31: Invitation handlers: GET/POST /invite/* (handlers/invite.rs) [deep]
├── Task 32: Stablecoin registry + Solana Pay service (services/solana_pay.rs) [deep]
├── Task 33: Payment handlers: initiate + status (handlers/payment.rs) [unspecified-high]
├── Task 34: Refund service: calculate + execute (services/refund.rs) [unspecified-high]
├── Task 35: cancel/expire agreement handlers + TX construction [deep]
└── Task 36: vote_revoke/retract/close agreement handlers + TX construction [deep]

Wave 7 (Workers — 4 parallel tasks):
├── Task 37: Event listener worker (workers/event_listener.rs) [deep]
├── Task 38: Keeper worker — 8 scan jobs (workers/keeper.rs) [deep]
├── Task 39: Refund worker (workers/refund_worker.rs) [unspecified-high]
└── Task 40: Notification worker (workers/notification_worker.rs) [unspecified-high]

Wave 8 (Docker + Integration — 4 parallel tasks):
├── Task 41: docker-compose.yml + Dockerfile [quick]
├── Task 42: main.rs final: wire all routes, spawn workers, startup validation [deep]
├── Task 43: Integration test suite: MVP end-to-end flow [deep]
└── Task 44: .gitignore + .env.example finalization + sqlx prepare [quick]

Wave FINAL (Verification — 4 parallel):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: Real manual QA (unspecified-high)
└── Task F4: Scope fidelity check (deep)

Critical Path: T1 → T5 → T11 → T14 → T15 → T18 → T19 → T21 → T42 → F1-F4
Parallel Speedup: ~75% faster than sequential
Max Concurrent: 7 (Waves 1 & 6)
```

### Dependency Matrix

| Task | Depends On | Blocks | Wave |
|------|-----------|--------|------|
| 1-7 | — | 8-13 | 1 |
| 8 | 1 | 26 | 2 |
| 9 | 1, 2 | 17, 31 | 2 |
| 10 | 1, 2, 5 | 18, 42 | 2 |
| 11 | 1, 2 | 14, 15, 16 | 2 |
| 12 | 1, 2, 5, 7 | 19, 20, 35, 36 | 2 |
| 13 | 1, 2, 5 | 40 | 2 |
| 14 | 11 | 15-18, 21-23 | 3 |
| 15 | 11, 14 | 18 | 3 |
| 16 | 11, 14 | 18 | 3 |
| 17 | 9, 14 | 18 | 3 |
| 18 | 5, 6, 10, 14-17 | 21-23 | 3 |
| 19-20 | 12 | 21, 22 | 4 |
| 21-23 | 14, 18, 19, 20 | 42 | 4 |
| 24 | 11, 14 | 42 | 5 |
| 25 | 14, 15 | 42 | 5 |
| 26 | 8, 14 | 30 | 5 |
| 27 | 1 | 26, 30 | 5 |
| 28 | 7 | 22 | 5 |
| 29 | 5, 14 | 37 | 5 |
| 30-31 | 9, 14, 27 | 33, 42 | 6 |
| 32-34 | 5, 12 | 33, 39 | 6 |
| 35-36 | 12, 14 | 37, 42 | 6 |
| 37-40 | 5, 29, 32, 34 | 42 | 7 |
| 41-44 | ALL | F1-F4 | 8 |
| F1-F4 | ALL | — | FINAL |

### Agent Dispatch Summary

| Wave | Tasks | Dispatches |
|------|-------|-----------|
| 1 | 7 | T1-T7 → `quick` |
| 2 | 6 | T8-T11,T13 → `quick`, T12 → `deep` |
| 3 | 5 | T14-T15 → `deep`, T16-T18 → `quick` |
| 4 | 5 | T19-T22 → `deep`, T23 → `unspecified-high` |
| 5 | 6 | T24 → `deep`, T25,T28 → `quick`, T26-T27,T29 → `unspecified-high` |
| 6 | 7 | T30-T32,T35-T36 → `deep`, T33-T34 → `unspecified-high` |
| 7 | 4 | T37-T38 → `deep`, T39-T40 → `unspecified-high` |
| 8 | 4 | T41,T44 → `quick`, T42-T43 → `deep` |
| FINAL | 4 | F1 → `oracle`, F2-F3 → `unspecified-high`, F4 → `deep` |

---

## TODOs

> Implementation + Test = ONE Task. Never separate.
> EVERY task MUST have: Recommended Agent Profile + Parallelization info + QA Scenarios.

---


- [x] 1. **Project Scaffolding + Cargo.toml + Test Infrastructure**

  **What to do**:
  - Create the full directory structure per spec §3: `src/`, `src/handlers/`, `src/services/`, `src/workers/`, `src/middleware/`, `migrations/`, `api/`
  - Write `Cargo.toml` exactly per spec §14 with all dependencies at specified versions
  - Create empty `mod.rs` / stub files for all modules to establish the module tree
  - Set up `src/main.rs` with `#[tokio::main]` skeleton that prints "Pactum backend starting..."
  - Create `tests/` directory with an `integration_test.rs` placeholder
  - Configure `sqlx` offline mode: add `.sqlx/` directory for prepared query metadata
  - Add `rust-toolchain.toml` pinning Rust edition 2021

  **Must NOT do**:
  - Do not implement any logic — stubs only
  - Do not use `lazy_static!` — use `std::sync::LazyLock` if needed

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`]
    - `coding-guidelines`: Rust naming conventions, module structure patterns
  - **Skills Evaluated but Omitted**:
    - `test-driven-development`: Not needed for scaffolding (no logic to test)

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2-7)
  - **Blocks**: Tasks 8-13 (all Wave 2 tasks)
  - **Blocked By**: None (can start immediately)

  **References**:
  - `pactum_backend_spec.md` lines 84-136 — exact project structure tree
  - `pactum_backend_spec.md` lines 2404-2474 — exact Cargo.toml content with all dependencies and versions
  - `pactum_backend_spec.md` lines 57-78 — technology stack table with exact crate versions
  - GitHub `PinkPlants/pactum/Cargo.toml` — workspace Cargo.toml pattern reference (release profile settings)

  **WHY Each Reference Matters**:
  - §3 structure tree: Must replicate exactly — every directory and file path is prescribed
  - §14 Cargo.toml: Dependencies must match exact versions — version mismatches cause compilation failures
  - Tech stack table: Cross-reference Cargo.toml versions against this table for consistency

  **Acceptance Criteria**:
  - [ ] `cargo check` succeeds (all stubs resolve)
  - [ ] Directory structure matches spec §3 exactly
  - [ ] All 24 crate dependencies present in Cargo.toml at correct versions

  **QA Scenarios:**
  ```
  Scenario: Project compiles with all dependencies
    Tool: Bash
    Preconditions: Cargo.toml created with all dependencies
    Steps:
      1. Run `cargo check` in project root
      2. Verify exit code is 0
      3. Run `find src -name '*.rs' | wc -l` and verify count >= 20
    Expected Result: cargo check succeeds, all stub files exist
    Failure Indicators: Compilation errors, missing module declarations
    Evidence: .sisyphus/evidence/task-1-cargo-check.txt

  Scenario: Directory structure matches spec
    Tool: Bash
    Preconditions: All directories and files created
    Steps:
      1. Run `ls -R src/` and compare against spec §3 structure
      2. Verify `migrations/` directory exists
      3. Verify `api/Dockerfile` placeholder exists
      4. Verify `docker-compose.yml` placeholder exists
    Expected Result: All directories and files from spec §3 present
    Evidence: .sisyphus/evidence/task-1-dir-structure.txt
  ```

  **Commit**: YES (groups with Wave 1)
  - Message: `feat: project scaffolding — Cargo.toml, directory structure, module stubs`
  - Files: `Cargo.toml`, `src/**/*.rs`, `migrations/`, `api/`, `tests/`
  - Pre-commit: `cargo check`

- [x] 2. **Error Types (error.rs) — Full AppError Enum**

  **What to do**:
  - Create `src/error.rs` with `AppError` enum using `thiserror`
  - Include ALL error variants referenced throughout the spec:
    - Auth: `InvalidOrExpiredNonce`, `InvalidRefreshToken`, `WalletRequired`, `EmailAlreadyRegistered`
    - Upload: `MissingContentType`, `InvalidFileType`, `FileTooLarge`, `HashMismatch`, `UploadFailed`
    - Agreement: `DraftNotReady`, `PaymentRequired`, `EmailRequired`, `InviteWindowExceedsSigningWindow`
    - Payment: `PaymentMethodUnsupported`, `TreasuryAtaMismatch`, `NoRefundAmountSet`
    - Crypto: `EncryptionFailed`, `DecryptionFailed`, `InvalidHash`
    - Keypair: `KeypairLoadFailed`, `VaultDepositExceedsMaximum`
    - Display name: `DisplayNameTooLong`, `InvalidDisplayName`
    - General: `Unauthorized`, `NotFound`, `InternalError`, `RateLimited`
  - Implement `IntoResponse` for `AppError` to return proper HTTP status codes + JSON error bodies
  - Write TDD tests: verify each error variant produces correct HTTP status code

  **Must NOT do**:
  - Do not use `anyhow` for handler errors — `thiserror` with typed variants only
  - Do not use `unwrap()` — propagate with `?`

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 3-7)
  - **Blocks**: Tasks 8-18 (everything depends on error types)
  - **Blocked By**: None

  **References**:
  - `pactum_backend_spec.md` lines 899-901 — Upload error responses (HashMismatch, UploadFailed)
  - `pactum_backend_spec.md` lines 640-642 — InvalidOrExpiredNonce error
  - `pactum_backend_spec.md` lines 730-749 — WalletRequired error with message and link_url
  - `pactum_backend_spec.md` lines 1139-1151 — DisplayNameTooLong, InvalidDisplayName
  - `pactum_backend_spec.md` lines 1371-1398 — PaymentRequired, DraftNotReady, EmailRequired errors

  **WHY Each Reference Matters**:
  - Each error variant has a specific HTTP status code and JSON payload shape defined in the spec
  - WalletRequired (§7.4) includes `message` + `link_url` — must be structured error, not just a string
  - PaymentRequired (§9.4) includes `draft_id` + `initiate_url` — handler-specific structured error

  **Acceptance Criteria**:
  - [ ] All 20+ AppError variants compile
  - [ ] `impl IntoResponse for AppError` returns correct HTTP status codes
  - [ ] `cargo test -- error` passes — each variant tested for correct status code

  **QA Scenarios:**
  ```
  Scenario: Error types compile and return correct HTTP status
    Tool: Bash
    Steps:
      1. Run `cargo test -- error`
      2. Verify all error variant tests pass
      3. Run `cargo clippy -- -D warnings` on error.rs
    Expected Result: All tests pass, zero clippy warnings
    Evidence: .sisyphus/evidence/task-2-error-tests.txt
  ```

  **Commit**: YES (groups with Wave 1)
  - Message: `feat(error): AppError enum with thiserror + IntoResponse impl`
  - Files: `src/error.rs`
  - Pre-commit: `cargo test -- error`

- [x] 3. **Configuration Module (config.rs + .env.example)**

  **What to do**:
  - Create `src/config.rs` with `Config` struct holding all ~60 env vars from spec §4
  - Use `dotenvy` for `.env` loading + `config` crate for typed config
  - Group config fields logically: database, solana, jwt, encryption, oauth, email, payment, storage, server
  - Include `StablecoinRegistry` struct with USDC/USDT/PYUSD mint addresses + ATAs
  - Create `.env.example` with all vars and comments from spec §4 (lines 142-226)
  - Add validation: panic at startup if required vars are missing
  - TDD: test config loading from env vars, test missing required var panics

  **Must NOT do**:
  - Do not hardcode any secrets — all from env/files
  - Do not store keypair bytes in Config — only file paths

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: Tasks 5, 10, 12, 32
  - **Blocked By**: None

  **References**:
  - `pactum_backend_spec.md` lines 142-226 — Full .env.example with all variables and comments
  - `pactum_backend_spec.md` lines 1215-1244 — StablecoinInfo + StablecoinRegistry structs
  - `pactum_backend_spec.md` lines 176-202 — Payment config vars (fee, free tier, keypair paths, thresholds)

  **WHY Each Reference Matters**:
  - §4 env vars: Every single variable must exist in both Config struct and .env.example
  - StablecoinRegistry: Payment handlers resolve method string to mint/ATA — registry must be built at startup
  - Payment config: Fee amounts, free tier limits, and safety thresholds must be configurable

  **Acceptance Criteria**:
  - [ ] `Config` struct has fields for all ~60 env vars
  - [ ] `.env.example` matches spec §4 exactly
  - [ ] `cargo test -- config` passes — loads from env, validates required vars

  **QA Scenarios:**
  ```
  Scenario: Config loads all required variables
    Tool: Bash
    Steps:
      1. Set minimal required env vars (DATABASE_URL, JWT_SECRET, etc.)
      2. Run `cargo test -- config::tests`
      3. Verify all config loading tests pass
    Expected Result: Config struct populated, missing var tests panic correctly
    Evidence: .sisyphus/evidence/task-3-config-tests.txt
  ```

  **Commit**: YES (groups with Wave 1)
  - Message: `feat(config): Config struct with all env vars + .env.example`
  - Files: `src/config.rs`, `.env.example`
  - Pre-commit: `cargo test -- config`

- [x] 4. **Database Migrations (13+ SQL Files)**

  **What to do**:
  - Create all migration SQL files in `migrations/` per spec §5:
    - `001_user_accounts.sql` — user_accounts table (§5.1)
    - `002_auth_wallet.sql` — auth_wallet table + index (§5.1)
    - `003_auth_oauth.sql` — auth_oauth table + index (§5.1)
    - `004_user_contacts.sql` — user_contacts table + email_index (§5.2)
    - `005_agreement_parties.sql` — agreement_parties table + 3 indexes (§5.3)
    - `006_notification_queue.sql` — notification_queue table + partial index (§5.4)
    - `007_agreement_drafts.sql` — agreement_drafts table + indexes + ALTER for paid/storage columns (§5.5 + §5.7)
    - `008_party_invitations.sql` — party_invitations table + 4 indexes (§5.6)
    - `009_agreement_payments.sql` — agreement_payments table + 4 indexes (§5.7)
    - `010_user_agreement_counts.sql` — user_agreement_counts table (§5.7)
    - `011_siws_nonces.sql` — siws_nonces table (§7.1)
    - `012_payment_tx_sig_unique.sql` — unique index on token_tx_signature (§9.3)
    - `013_refresh_tokens.sql` — refresh_tokens table + index (§7.5)
  - Copy SQL exactly from spec — use BIGINT for timestamps, gen_random_uuid(), epoch from now()
  - Verify all foreign key references are correct
  - TDD: test that migrations apply cleanly to a fresh PostgreSQL instance

  **Must NOT do**:
  - Do not modify the SQL from spec — copy verbatim
  - Do not add ORM abstractions — raw SQL migrations only

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: Tasks 14-40 (all handler/worker tasks need DB schema)
  - **Blocked By**: None

  **References**:
  - `pactum_backend_spec.md` lines 271-302 — §5.1 user_accounts, auth_wallet, auth_oauth
  - `pactum_backend_spec.md` lines 306-322 — §5.2 user_contacts (PII encrypted)
  - `pactum_backend_spec.md` lines 326-343 — §5.3 agreement_parties
  - `pactum_backend_spec.md` lines 347-362 — §5.4 notification_queue
  - `pactum_backend_spec.md` lines 368-392 — §5.5 agreement_drafts
  - `pactum_backend_spec.md` lines 424-446 — §5.6 party_invitations
  - `pactum_backend_spec.md` lines 470-531 — §5.7 agreement_payments + user_agreement_counts
  - `pactum_backend_spec.md` lines 606-613 — §7.1 siws_nonces
  - `pactum_backend_spec.md` lines 766-773 — §7.5 refresh_tokens
  - `pactum_backend_spec.md` lines 1342-1347 — §9.3 payment_tx_sig_unique index

  **Acceptance Criteria**:
  - [ ] 13 migration files exist in `migrations/`
  - [ ] SQL syntax is valid PostgreSQL 16
  - [ ] `sqlx migrate run` succeeds against a clean PostgreSQL instance

  **QA Scenarios:**
  ```
  Scenario: Migrations apply cleanly
    Tool: Bash
    Preconditions: PostgreSQL 16 running (docker-compose test)
    Steps:
      1. Run `sqlx database create` to create test DB
      2. Run `sqlx migrate run`
      3. Run `psql -c '\dt' $DATABASE_URL` to list all tables
      4. Verify 10+ tables exist (user_accounts, auth_wallet, etc.)
    Expected Result: All 13 migrations applied, 10+ tables created, all indexes present
    Failure Indicators: SQL syntax errors, foreign key violations
    Evidence: .sisyphus/evidence/task-4-migrations.txt
  ```

  **Commit**: YES (groups with Wave 1)
  - Message: `feat(db): all 13 PostgreSQL migrations per spec §5`
  - Files: `migrations/*.sql`
  - Pre-commit: `sqlx migrate run` (if DB available)

- [x] 5. **AppState + ProtectedKeypair (state.rs)**

  **What to do**:
  - Create `src/state.rs` with `AppState` struct exactly per spec §4 (lines 248-262)
  - Implement `ProtectedKeypair` newtype with Debug/Display that redact keypair bytes
  - `AppState` fields: `db` (PgPool), `config` (Arc<Config>), `solana` (Arc<RpcClient>), `vault_keypair` (Arc<ProtectedKeypair>), `treasury_keypair` (Arc<ProtectedKeypair>), `ws_channels` (Arc<DashMap<Uuid, broadcast::Sender<WsEvent>>>)
  - Define `WsEvent` enum with all event types from spec §10.2
  - Implement `Clone` for `AppState` (all fields are Arc-wrapped or Clone)
  - TDD: test ProtectedKeypair Debug/Display redacts, test AppState Clone compiles

  **Must NOT do**:
  - Do not expose keypair bytes in any Debug/Display impl
  - Do not use `println!` — only `tracing` macros

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: Tasks 10, 12, 13, 14, 18, 29, 37-42
  - **Blocked By**: None

  **References**:
  - `pactum_backend_spec.md` lines 230-262 — AppState struct with ProtectedKeypair
  - `pactum_backend_spec.md` lines 1605-1677 — WsEvent types and channel architecture
  - `pactum_backend_spec.md` lines 1618-1631 — Event type table (10 events)

  **Acceptance Criteria**:
  - [ ] `AppState` struct compiles with all fields
  - [ ] `ProtectedKeypair` Debug output is `ProtectedKeypair([REDACTED])`
  - [ ] `WsEvent` enum includes all 10 event types

  **QA Scenarios:**
  ```
  Scenario: ProtectedKeypair never leaks key material
    Tool: Bash
    Steps:
      1. Run `cargo test -- state::tests`
      2. Verify ProtectedKeypair Debug is "ProtectedKeypair([REDACTED])"
      3. Verify ProtectedKeypair Display is "[REDACTED]"
    Expected Result: Tests pass, no key material in output
    Evidence: .sisyphus/evidence/task-5-state-tests.txt
  ```

  **Commit**: YES (groups with Wave 1)
  - Message: `feat(state): AppState with ProtectedKeypair + WsEvent types`
  - Files: `src/state.rs`
  - Pre-commit: `cargo test -- state`

- [x] 6. **Router Skeleton + Middleware Stack (router.rs)**

  **What to do**:
  - Create `src/router.rs` with `build_router(state: AppState) -> Router` exactly per spec §6
  - Configure middleware stack in correct order: CORS → Rate Limiting → Tracing
  - CORS: whitelist `https://pactum.app` and `https://app.pactum.app`; allow GET/POST/PUT/DELETE; allow Authorization + Content-Type headers
  - Rate limiting: GovernorLayer with per-IP SmartIpKeyExtractor, 100 req/min general
  - Tracing: tower-http TraceLayer for structured request logs
  - Define empty route group functions: `auth_routes()`, `upload_routes()`, `agreement_routes()`, `draft_routes()`, `invite_routes()`, `payment_routes()`, `user_routes()`, `ws_routes()` — return empty Routers for now
  - Use Axum 0.8 path syntax: `/{param}` not `/:param`
  - TDD: test that router builds without panic, test CORS headers present

  **Must NOT do**:
  - Do not use `CorsLayer::permissive()` — configure specific origins

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: Tasks 14-18, 42
  - **Blocked By**: None

  **References**:
  - `pactum_backend_spec.md` lines 557-588 — §6 router.rs with middleware stack code
  - `pactum_backend_spec.md` lines 540-555 — Middleware order diagram

  **Acceptance Criteria**:
  - [ ] `build_router()` compiles and returns a Router
  - [ ] CORS configured with specific origins (not permissive)
  - [ ] GovernorLayer configured with SmartIpKeyExtractor

  **QA Scenarios:**
  ```
  Scenario: Router builds with middleware stack
    Tool: Bash
    Steps:
      1. Run `cargo test -- router::tests`
      2. Verify router construction doesn't panic
    Expected Result: Tests pass
    Evidence: .sisyphus/evidence/task-6-router-tests.txt
  ```

  **Commit**: YES (groups with Wave 1)
  - Message: `feat(router): skeleton with CORS, rate limiting, tracing middleware`
  - Files: `src/router.rs`

- [x] 7. **Solana Constants + Types Module**

  **What to do**:
  - Create `src/solana_types.rs` (or `src/types/`) with Rust types mirroring the on-chain program:
    - `AgreementStatus` enum: Draft, PendingSignatures, Completed, Cancelled, Expired, Revoked
    - `StorageBackend` enum: Ipfs, Arweave
    - `CreateAgreementArgs` struct matching on-chain args (agreement_id, title, content_hash, storage_uri, storage_backend, parties, vault_deposit, expires_in_secs)
    - `SignAgreementArgs` struct: metadata_uri (Option<String>)
  - Include on-chain constants: MAX_PARTIES=10, MAX_EXPIRY_SECONDS=7776000, MAX_URI_LEN=128, MAX_TITLE_LEN=64, VAULT_BUFFER=10_000_000
  - Define the program ID as a constant: `DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P`
  - All types derive `Serialize, Deserialize, Clone, Debug` for API use
  - Implement `BorshSerialize` for args that need to be serialized for on-chain instructions
  - TDD: test serialization roundtrips, test constant values match on-chain

  **Must NOT do**:
  - Do not depend on `anchor-lang` or `anchor-client` — these types are backend-side copies

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: Tasks 12, 19-23, 28, 35-36
  - **Blocked By**: None

  **References**:
  - GitHub `PinkPlants/pactum/programs/pactum/src/state/mod.rs` — AgreementState, AgreementStatus, StorageBackend, AGREEMENT_STATE_SIZE
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/create_agreement.rs` — CreateAgreementArgs struct
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/sign_agreement.rs` — SignAgreementArgs struct
  - GitHub `PinkPlants/pactum/programs/pactum/src/constants.rs` — MAX_PARTIES, MAX_EXPIRY_SECONDS, MAX_URI_LEN, MAX_TITLE_LEN, VAULT_BUFFER
  - GitHub `PinkPlants/pactum/programs/pactum/src/lib.rs` — Program ID declaration

  **WHY Each Reference Matters**:
  - state/mod.rs: Backend types must match on-chain enum variants exactly or borsh serialization will produce wrong discriminators
  - CreateAgreementArgs: Field order matters for borsh — must match exactly
  - constants.rs: Backend validation must use same limits as on-chain program

  **Acceptance Criteria**:
  - [ ] All on-chain types replicated with correct borsh serialization
  - [ ] Program ID constant matches `DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P`
  - [ ] `cargo test -- solana_types` passes

  **QA Scenarios:**
  ```
  Scenario: Borsh serialization matches on-chain format
    Tool: Bash
    Steps:
      1. Run `cargo test -- solana_types::tests`
      2. Verify CreateAgreementArgs serializes to expected byte format
      3. Verify AgreementStatus enum discriminators match on-chain (0=Draft, 1=PendingSignatures, etc.)
    Expected Result: All serialization tests pass
    Evidence: .sisyphus/evidence/task-7-types-tests.txt
  ```

  **Commit**: YES (groups with Wave 1)
  - Message: `feat(types): Solana on-chain types, constants, program ID`
  - Files: `src/solana_types.rs`

- [x] 8. **SHA-256 Hash Service (services/hash.rs)**

  **What to do**:
  - Create `src/services/hash.rs` with `compute_sha256()` and `verify_client_hash()` exactly per spec §11.1
  - `compute_sha256(data: &[u8]) -> [u8; 32]` using `sha2::Sha256`
  - `verify_client_hash(file_bytes, client_hash_hex) -> Result<[u8; 32], AppError>` — hex decode client hash, compare with server hash, return `AppError::HashMismatch` on mismatch, `AppError::InvalidHash` on bad hex
  - Add `hex` crate for hex encoding/decoding (already in Cargo.toml)
  - TDD: test correct hash match, test mismatch returns error, test invalid hex returns error

  **Must NOT do**:
  - Do not use any other hashing algorithm — SHA-256 only

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 9-13)
  - **Blocks**: Task 26 (upload handler needs hash verification)
  - **Blocked By**: Task 1 (project scaffolding)

  **References**:
  - `pactum_backend_spec.md` lines 1697-1717 — §11.1 exact hash verification code
  - `pactum_backend_spec.md` lines 899-900 — HashMismatch + InvalidHash error usage

  **WHY Each Reference Matters**:
  - §11.1: Copy the function signatures exactly — upload handler calls these directly
  - Error references: Ensure AppError variants match what upload handler expects

  **Acceptance Criteria**:
  - [ ] `compute_sha256()` returns correct SHA-256 for known test vectors
  - [ ] `verify_client_hash()` returns Ok on match, Err(HashMismatch) on mismatch
  - [ ] `cargo test -- hash` passes

  **QA Scenarios:**
  ```
  Scenario: Hash computation matches known test vector
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::hash::tests`
      2. Verify SHA-256 of "hello world" matches known digest e3b0c44298fc1c149afbf4c8996fb924...
      3. Verify mismatched hash returns AppError::HashMismatch
    Expected Result: All hash tests pass
    Evidence: .sisyphus/evidence/task-8-hash-tests.txt

  Scenario: Invalid hex input returns InvalidHash
    Tool: Bash
    Steps:
      1. Run `cargo test -- hash::tests::test_invalid_hex`
      2. Verify "not-valid-hex" input returns AppError::InvalidHash
    Expected Result: Error variant is InvalidHash, not a panic
    Evidence: .sisyphus/evidence/task-8-invalid-hex.txt
  ```

  **Commit**: YES (groups with Wave 2)
  - Message: `feat(hash): SHA-256 compute + verify service`
  - Files: `src/services/hash.rs`
  - Pre-commit: `cargo test -- hash`

- [x] 9. **AES-256-GCM Crypto Service (services/crypto.rs)**

  **What to do**:
  - Create `src/services/crypto.rs` with `encrypt()`, `decrypt()`, and `hmac_index()` exactly per spec §11.2
  - `encrypt(plaintext: &str, key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12]), AppError>` — random nonce via OsRng, AES-256-GCM encryption
  - `decrypt(ciphertext: &[u8], nonce: &[u8; 12], key: &[u8; 32]) -> Result<String, AppError>` — decrypt + UTF-8 validation
  - `hmac_index(value: &str, key: &[u8; 32]) -> Vec<u8>` — HMAC-SHA256 blind index for email lookup
  - CRITICAL: NEVER reuse nonces — use `OsRng.fill_bytes()` for every encryption
  - TDD: test encrypt→decrypt roundtrip, test wrong key fails, test HMAC deterministic for same input

  **Must NOT do**:
  - Do not reuse nonces — always generate fresh random nonces
  - Do not use `unwrap()` — propagate with `?`

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 8, 10-13)
  - **Blocks**: Tasks 17 (user contacts need encryption), 31 (invitations need encrypted email)
  - **Blocked By**: Tasks 1 (scaffolding), 2 (error types)

  **References**:
  - `pactum_backend_spec.md` lines 1719-1750 — §11.2 exact encrypt/decrypt/hmac_index code
  - `pactum_backend_spec.md` lines 306-322 — §5.2 user_contacts encrypted fields
  - `pactum_backend_spec.md` lines 428-430 — §5.6 party_invitations encrypted email fields

  **WHY Each Reference Matters**:
  - §11.2: Function signatures and implementations are prescribed — copy exactly
  - §5.2 + §5.6: These tables store ciphertext + nonce separately — crypto service must output in compatible format

  **Acceptance Criteria**:
  - [ ] encrypt→decrypt roundtrip produces original plaintext
  - [ ] Wrong key produces `AppError::DecryptionFailed`
  - [ ] `hmac_index("test@email.com", key)` is deterministic (same input → same output)
  - [ ] `cargo test -- crypto` passes

  **QA Scenarios:**
  ```
  Scenario: Encrypt-decrypt roundtrip
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::crypto::tests`
      2. Verify encrypt("hello@example.com") → decrypt → "hello@example.com"
      3. Verify two encryptions of same plaintext produce different ciphertexts (random nonce)
    Expected Result: All crypto tests pass, nonces are unique
    Evidence: .sisyphus/evidence/task-9-crypto-tests.txt

  Scenario: Wrong key decryption fails gracefully
    Tool: Bash
    Steps:
      1. Run `cargo test -- crypto::tests::test_wrong_key`
      2. Verify returns Err(AppError::DecryptionFailed), not a panic
    Expected Result: Graceful error, no panic
    Evidence: .sisyphus/evidence/task-9-wrong-key.txt
  ```

  **Commit**: YES (groups with Wave 2)
  - Message: `feat(crypto): AES-256-GCM encrypt/decrypt + HMAC blind index`
  - Files: `src/services/crypto.rs`
  - Pre-commit: `cargo test -- crypto`

- [x] 10. **Keypair Security Service (services/keypair_security.rs)**

  **What to do**:
  - Create `src/services/keypair_security.rs` per spec §11.5
  - `load_keypair(path: &str) -> Result<ProtectedKeypair, AppError>` — read JSON file, parse [u8;64], construct Keypair, wrap in ProtectedKeypair
  - `validate_keypair_pubkeys(state: &AppState)` — assert vault and treasury pubkeys match config values, panic on mismatch with descriptive message
  - Handle errors: `AppError::KeypairLoadFailed(String)` for file-not-found, invalid JSON, invalid bytes
  - TDD: test load from valid JSON, test invalid JSON returns error, test pubkey mismatch panics

  **Must NOT do**:
  - Do not log keypair bytes under any circumstances
  - Do not store keypair bytes in environment variables — file paths only

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 8-9, 11-13)
  - **Blocks**: Tasks 18 (main.rs needs keypair loading), 42 (final wiring)
  - **Blocked By**: Tasks 1 (scaffolding), 2 (error types), 5 (ProtectedKeypair type)

  **References**:
  - `pactum_backend_spec.md` lines 1879-1928 — §11.5 exact load_keypair + validate_keypair_pubkeys code
  - `pactum_backend_spec.md` lines 1883-1890 — Threat model table
  - `pactum_backend_spec.md` lines 1930-1953 — Secret storage options + rotation procedure

  **WHY Each Reference Matters**:
  - §11.5: load_keypair reads Solana-format [u8;64] JSON — not base58, not PEM
  - Validation function: assert_eq with descriptive messages catches wrong-file-loaded at startup

  **Acceptance Criteria**:
  - [ ] `load_keypair()` loads a valid Solana keypair JSON file
  - [ ] `load_keypair()` returns `KeypairLoadFailed` for invalid files
  - [ ] `validate_keypair_pubkeys()` panics with descriptive message on mismatch
  - [ ] `cargo test -- keypair_security` passes

  **QA Scenarios:**
  ```
  Scenario: Load valid keypair file
    Tool: Bash
    Steps:
      1. Create a temp keypair JSON file with `solana-keygen new --no-bip39-passphrase -o /tmp/test_kp.json`
      2. Run `cargo test -- keypair_security::tests::test_load_valid`
      3. Verify loaded pubkey matches expected
    Expected Result: Keypair loads successfully, pubkey matches
    Evidence: .sisyphus/evidence/task-10-load-keypair.txt

  Scenario: Invalid file returns error
    Tool: Bash
    Steps:
      1. Run `cargo test -- keypair_security::tests::test_invalid_file`
      2. Verify returns Err(AppError::KeypairLoadFailed(_))
    Expected Result: Graceful error with descriptive message
    Evidence: .sisyphus/evidence/task-10-invalid-file.txt
  ```

  **Commit**: YES (groups with Wave 2)
  - Message: `feat(keypair): secure keypair loading + startup pubkey validation`
  - Files: `src/services/keypair_security.rs`
  - Pre-commit: `cargo test -- keypair_security`

- [x] 11. **JWT Encode/Decode/Validate Utilities**

  **What to do**:
  - Create JWT utility functions (in `src/services/` or `src/middleware/`):
  - `issue_access_token(user_id: Uuid, pubkey: Option<String>, config: &Config) -> Result<String, AppError>` — encode Claims with `exp = now + JWT_ACCESS_EXPIRY_SECONDS` (900s), `iat = now`, `jti = Uuid::new_v4()`
  - `issue_and_store_refresh_token(db: &PgPool, user_id: Uuid) -> Result<String, AppError>` — generate random token, SHA-256 hash it, store hash in `refresh_tokens` table, return raw token
  - `decode_access_token(token: &str, config: &Config) -> Result<Claims, AppError>` — validate exp, return Claims struct
  - `sha256_hex(input: &str) -> String` — hex-encoded SHA-256 for refresh token hashing
  - Claims struct per spec §7.4: `sub: Uuid, pubkey: Option<String>, exp: usize, iat: usize, jti: Uuid`
  - TDD: test issue + decode roundtrip, test expired token fails, test refresh token hash stored correctly

  **Must NOT do**:
  - Do not store plaintext refresh tokens — always SHA-256 hash before storage
  - Do not use milliseconds for exp — `jsonwebtoken` expects seconds

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 8-10, 12-13)
  - **Blocks**: Tasks 14 (JWT middleware), 15 (SIWS auth), 16 (refresh/logout)
  - **Blocked By**: Tasks 1 (scaffolding), 2 (error types)

  **References**:
  - `pactum_backend_spec.md` lines 710-726 — §7.4 JWT Claims struct
  - `pactum_backend_spec.md` lines 783-800 — §7.5 refresh flow (sha256_hex, delete-on-use, issue_and_store)
  - `pactum_backend_spec.md` lines 150-152 — JWT_ACCESS_EXPIRY_SECONDS (900), JWT_REFRESH_EXPIRY_SECONDS (604800)
  - `pactum_backend_spec.md` lines 766-773 — refresh_tokens table schema

  **WHY Each Reference Matters**:
  - §7.4 Claims: Field names and types must match exactly — pubkey is Option<String>, exp is usize (seconds)
  - §7.5: Refresh token rotation is delete-on-use — issue new on every refresh
  - Expiry config: Access = 900s, Refresh = 604800s — hardcoded defaults with config override

  **Acceptance Criteria**:
  - [ ] Access token encodes + decodes with correct Claims fields
  - [ ] Expired access token returns `Err(AppError::Unauthorized)`
  - [ ] Refresh token is SHA-256 hashed before storage
  - [ ] `cargo test -- jwt` passes

  **QA Scenarios:**
  ```
  Scenario: JWT roundtrip
    Tool: Bash
    Steps:
      1. Run `cargo test -- jwt::tests`
      2. Verify access token encode→decode preserves user_id and pubkey
      3. Verify expired token decode returns error
    Expected Result: All JWT tests pass
    Evidence: .sisyphus/evidence/task-11-jwt-tests.txt

  Scenario: Refresh token hash storage
    Tool: Bash
    Steps:
      1. Run `cargo test -- jwt::tests::test_refresh_hash`
      2. Verify sha256_hex("test_token") matches known SHA-256 hex
      3. Verify plaintext token is never equal to stored hash
    Expected Result: Hash is correct, plaintext never stored
    Evidence: .sisyphus/evidence/task-11-refresh-hash.txt
  ```

  **Commit**: YES (groups with Wave 2)
  - Message: `feat(jwt): access/refresh token issue + decode + sha256_hex`
  - Files: `src/services/jwt.rs` (or `src/middleware/jwt.rs`)
  - Pre-commit: `cargo test -- jwt`

- [x] 12. **Solana Service Foundation: PDA, Discriminators, RPC Wrapper (services/solana.rs)**

  **What to do**:
  - Create `src/services/solana.rs` with core Solana utilities:
  - PDA derivation functions:
    - `derive_agreement_pda(creator: &Pubkey, agreement_id: &[u8; 16]) -> (Pubkey, u8)` — seeds: `[b"agreement", creator.as_ref(), agreement_id]`
    - `derive_mint_vault_pda(agreement: &Pubkey) -> (Pubkey, u8)` — seeds: `[b"mint_vault", agreement.as_ref()]`
    - `derive_pda_authority() -> (Pubkey, u8)` — seeds: `[b"mint_authority", b"v1"]`
  - Anchor discriminator computation:
    - `fn compute_discriminator(name: &str) -> [u8; 8]` — SHA-256("global:<name>")[0..8]
    - Pre-computed constants for all 8 instructions: create_agreement, sign_agreement, cancel_agreement, expire_agreement, vote_revoke, retract_revoke_vote, close_agreement, initialize_collection
  - Instruction builder:
    - `fn build_anchor_instruction(program_id: &Pubkey, discriminator: &[u8; 8], accounts: Vec<AccountMeta>, args_data: &[u8]) -> Instruction`
  - Vault deposit calculation:
    - `async fn calculate_vault_deposit(rpc: &RpcClient) -> Result<u64, AppError>` — `getMinimumBalanceForRentExemption(AGREEMENT_STATE_SIZE) + VAULT_BUFFER`
  - TDD: test PDA derivations match known values, test discriminators match Anchor-generated values

  **Must NOT do**:
  - Do not use `anchor-client` — manual instruction construction only
  - Do not sign transactions on behalf of users

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]
    - `deep`: Complex Solana + Anchor interaction requiring careful byte-level correctness

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 8-11, 13)
  - **Blocks**: Tasks 19, 20, 35, 36 (all TX construction tasks)
  - **Blocked By**: Tasks 1 (scaffolding), 2 (error types), 5 (AppState), 7 (solana types)

  **References**:
  - `pactum_backend_spec.md` lines 1752-1808 — §11.3 transaction construction with build_create_agreement_tx and validate
  - GitHub `PinkPlants/pactum/programs/pactum/src/lib.rs` — Program ID, instruction definitions
  - GitHub `PinkPlants/pactum/programs/pactum/src/constants.rs` — VAULT_BUFFER, AGREEMENT_STATE_SIZE
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/create_agreement.rs` — Account ordering for create_agreement instruction
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/sign_agreement.rs` — Account ordering for sign_agreement
  - Draft notes: PDA seeds `[b"agreement", creator, agreement_id]`, `[b"mint_vault", agreement]`, `[b"mint_authority", b"v1"]`

  **WHY Each Reference Matters**:
  - §11.3: Exact transaction validation logic — vault deposit ceiling prevents drain attacks
  - On-chain lib.rs: Account ordering must match exactly — wrong order = Anchor deserialization failure
  - constants.rs: VAULT_BUFFER (10_000_000 lamports) must match on-chain

  **Acceptance Criteria**:
  - [ ] PDA derivations match on-chain program (test with known seeds)
  - [ ] All 8 instruction discriminators match Anchor-computed values
  - [ ] `build_anchor_instruction()` produces valid Instruction struct
  - [ ] `cargo test -- services::solana` passes

  **QA Scenarios:**
  ```
  Scenario: PDA derivation matches on-chain
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::solana::tests::test_pda_derivation`
      2. Verify agreement PDA for known creator + agreement_id matches expected address
      3. Verify mint_vault PDA derives correctly
      4. Verify pda_authority derives correctly
    Expected Result: All PDA addresses match expected values
    Evidence: .sisyphus/evidence/task-12-pda-tests.txt

  Scenario: Anchor discriminators are correct
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::solana::tests::test_discriminators`
      2. Verify SHA-256("global:create_agreement")[0..8] = expected bytes
      3. Verify all 8 instruction discriminators computed correctly
    Expected Result: All discriminators match Anchor-generated values
    Evidence: .sisyphus/evidence/task-12-discriminators.txt
  ```

  **Commit**: YES (groups with Wave 2)
  - Message: `feat(solana): PDA derivation, Anchor discriminators, instruction builder`
  - Files: `src/services/solana.rs`
  - Pre-commit: `cargo test -- services::solana`

- [x] 13. **Notification Service Skeleton (services/notification.rs)**

  **What to do**:
  - Create `src/services/notification.rs` with:
  - `enqueue_notification(db: &PgPool, event_type: &str, agreement_pda: &str, recipient_pubkey: &str) -> Result<(), AppError>` — INSERT into notification_queue
  - `fetch_pending_jobs(db: &PgPool, limit: i64) -> Vec<NotificationJob>` — SELECT WHERE status = 'pending' ORDER BY scheduled_at LIMIT $1
  - `mark_sent(db: &PgPool, id: Uuid)` — UPDATE status = 'sent'
  - `increment_attempts(db: &PgPool, id: Uuid)` — UPDATE attempts = attempts + 1
  - `NotificationEvent` enum with all 13 event types from spec §12.4
  - Email dispatch skeleton: `send_email(state, job, contact) -> Result<(), AppError>` — using `resend-rs` to send via Resend API
  - WS broadcast helper: `build_ws_event(job: &NotificationJob) -> WsEvent`
  - TDD: test enqueue inserts row, test fetch returns pending jobs only

  **Must NOT do**:
  - Do not implement full email templates yet — skeleton with event type + PDA is sufficient
  - Do not use `println!` — use `tracing` macros

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 8-12)
  - **Blocks**: Task 40 (notification worker)
  - **Blocked By**: Tasks 1 (scaffolding), 2 (error types), 5 (AppState)

  **References**:
  - `pactum_backend_spec.md` lines 2236-2277 — §12.3 notification_worker dispatch logic
  - `pactum_backend_spec.md` lines 2279-2295 — §12.4 notification event types table (13 events)
  - `pactum_backend_spec.md` lines 347-362 — §5.4 notification_queue table schema

  **WHY Each Reference Matters**:
  - §12.3: Dispatch logic — WS first, email if available, skip if no contact
  - §12.4: Event types determine email subject lines — enum must include all 13
  - §5.4: notification_queue columns must match INSERT/SELECT queries

  **Acceptance Criteria**:
  - [ ] `NotificationEvent` enum has all 13 event types
  - [ ] `enqueue_notification()` compiles with correct INSERT query
  - [ ] `fetch_pending_jobs()` compiles with correct SELECT query
  - [ ] `cargo test -- notification` passes

  **QA Scenarios:**
  ```
  Scenario: Notification queue operations
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::notification::tests`
      2. Verify enqueue creates a notification_queue row with status='pending'
      3. Verify fetch_pending returns only pending jobs ordered by scheduled_at
    Expected Result: All notification service tests pass
    Evidence: .sisyphus/evidence/task-13-notification-tests.txt
  ```

  **Commit**: YES (groups with Wave 2)
  - Message: `feat(notification): notification service skeleton with queue operations + 13 event types`
  - Files: `src/services/notification.rs`
  - Pre-commit: `cargo test -- notification`


- [x] 14. **JWT Middleware Extractor: AuthUser + AuthUserWithWallet (middleware/)**

  **What to do**:
  - Create `src/middleware/auth.rs` with `AuthUser` extractor per spec §7.4:
    - Implement `FromRequestParts` for `AuthUser`: extract `Authorization: Bearer <token>`, decode JWT, return `AuthUser { user_id, pubkey }`
    - On missing/invalid token: return `AppError::Unauthorized`
  - Create `src/middleware/wallet_guard.rs` with `AuthUserWithWallet` per spec §7.4:
    - Implement `FromRequestParts` for `AuthUserWithWallet`: extract AuthUser, then require `pubkey.is_some()`
    - On `pubkey == None`: return `AppError::WalletRequired { message, link_url }`
  - TDD: test valid token extracts correctly, test expired token rejected, test missing pubkey returns WalletRequired

  **Must NOT do**:
  - Do not implement `#[async_trait]` — Axum 0.8 handles async extractors natively

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]
    - `deep`: Axum extractor implementation requires understanding of `FromRequestParts` trait

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 15-18)
  - **Blocks**: Tasks 15-18, 21-29, 30-36 (all handler tasks need auth)
  - **Blocked By**: Task 11 (JWT utilities)

  **References**:
  - `pactum_backend_spec.md` lines 710-750 — §7.4 AuthUser, Claims, AuthUserWithWallet code
  - `pactum_backend_spec.md` lines 728-750 — Wallet guard middleware exact implementation

  **WHY Each Reference Matters**:
  - §7.4: `AuthUser.pubkey` is `Option<String>` — OAuth users have None until they link a wallet
  - WalletRequired error includes `link_url: "/auth/link/wallet"` — frontend uses this to redirect

  **Acceptance Criteria**:
  - [ ] `AuthUser` extracts user_id and pubkey from valid JWT
  - [ ] `AuthUserWithWallet` rejects users without pubkey with `WalletRequired`
  - [ ] `cargo test -- middleware` passes

  **QA Scenarios:**
  ```
  Scenario: AuthUser extracts valid JWT
    Tool: Bash
    Steps:
      1. Run `cargo test -- middleware::auth::tests`
      2. Verify valid JWT extracts correct user_id and pubkey
      3. Verify expired JWT returns Unauthorized
      4. Verify missing Authorization header returns Unauthorized
    Expected Result: All middleware auth tests pass
    Evidence: .sisyphus/evidence/task-14-auth-middleware.txt

  Scenario: WalletRequired for OAuth-only users
    Tool: Bash
    Steps:
      1. Run `cargo test -- middleware::wallet_guard::tests`
      2. Verify JWT with pubkey=None returns WalletRequired with link_url
      3. Verify JWT with pubkey=Some("...") extracts correctly
    Expected Result: WalletRequired includes message and link_url
    Evidence: .sisyphus/evidence/task-14-wallet-guard.txt
  ```

  **Commit**: YES (groups with Wave 3)
  - Message: `feat(middleware): JWT AuthUser + AuthUserWithWallet extractors`
  - Files: `src/middleware/auth.rs`, `src/middleware/wallet_guard.rs`
  - Pre-commit: `cargo test -- middleware`

- [x] 15. **SIWS Auth: Challenge + Verify (handlers/auth.rs - SIWS)**

  **What to do**:
  - Implement `GET /auth/challenge` handler per spec §7.1:
    - Generate UUID nonce, INSERT into siws_nonces table, return `{ "nonce": "uuid" }`
  - Implement `POST /auth/verify` handler per spec §7.1:
    - Consume nonce atomically: `DELETE FROM siws_nonces WHERE nonce = $1 AND created_at > now() - 300 RETURNING nonce`
    - If no row: return `AppError::InvalidOrExpiredNonce`
    - Verify ed25519 signature: `sig.verify(pubkey_bytes, nonce_bytes)` using `solana_sdk::signature::Signature`
    - Upsert `user_accounts` + `auth_wallet` (implicit signup on first login)
    - Return `{ access_token, refresh_token }` using JWT utilities from Task 11
  - TDD: test challenge returns UUID, test verify with valid sig returns tokens, test expired nonce rejected, test invalid sig rejected

  **Must NOT do**:
  - Do not use in-memory nonce storage — PostgreSQL only (multi-instance safe)
  - Do not skip nonce consumption — DELETE atomically prevents replay

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 14, 16-18)
  - **Blocks**: Task 18 (main.rs wiring), Task 25 (wallet linking)
  - **Blocked By**: Tasks 11 (JWT), 14 (auth middleware)

  **References**:
  - `pactum_backend_spec.md` lines 594-644 — §7.1 complete SIWS flow with code
  - `pactum_backend_spec.md` lines 604-613 — siws_nonces table + keeper cleanup note
  - `pactum_backend_spec.md` lines 271-302 — §5.1 user_accounts + auth_wallet tables

  **WHY Each Reference Matters**:
  - §7.1: Atomic nonce consumption prevents replay attacks — must use DELETE...RETURNING, not SELECT then DELETE
  - §5.1: Upsert creates user_accounts + auth_wallet on first login — implicit signup

  **Acceptance Criteria**:
  - [ ] `GET /auth/challenge` returns `{ "nonce": "<uuid>" }` and inserts into siws_nonces
  - [ ] `POST /auth/verify` consumes nonce atomically, verifies sig, returns tokens
  - [ ] Expired nonce (>300s) returns `InvalidOrExpiredNonce`
  - [ ] `cargo test -- handlers::auth::tests::siws` passes

  **QA Scenarios:**
  ```
  Scenario: SIWS auth flow
    Tool: Bash
    Preconditions: PostgreSQL running with migrations applied
    Steps:
      1. Run `cargo test -- handlers::auth::tests::test_siws_challenge`
      2. Verify nonce is UUID format and exists in siws_nonces table
      3. Run `cargo test -- handlers::auth::tests::test_siws_verify`
      4. Verify valid signature returns access_token + refresh_token
    Expected Result: Complete SIWS flow works
    Evidence: .sisyphus/evidence/task-15-siws-auth.txt

  Scenario: Expired nonce rejection
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::auth::tests::test_expired_nonce`
      2. Verify nonce older than 300s returns InvalidOrExpiredNonce
    Expected Result: 401 with InvalidOrExpiredNonce error
    Evidence: .sisyphus/evidence/task-15-expired-nonce.txt
  ```

  **Commit**: YES (groups with Wave 3)
  - Message: `feat(auth): SIWS challenge + verify with atomic nonce consumption`
  - Files: `src/handlers/auth.rs`
  - Pre-commit: `cargo test -- handlers::auth`

- [x] 16. **Token Refresh + Logout (handlers/auth.rs - refresh)**

  **What to do**:
  - Implement `POST /auth/refresh` handler per spec §7.5:
    - SHA-256 hash the incoming refresh_token
    - `DELETE FROM refresh_tokens WHERE token_hash = $1 AND expires_at > now() RETURNING user_id`
    - If no row: return `AppError::InvalidRefreshToken`
    - Fetch user's current pubkey from auth_wallet (may have changed since last token)
    - Issue new access_token + rotate refresh_token (delete-on-use)
  - Implement `POST /auth/logout` handler per spec §7.5:
    - SHA-256 hash the refresh_token
    - `DELETE FROM refresh_tokens WHERE token_hash = $1`
    - Return 204 No Content
  - TDD: test refresh returns new tokens, test invalid refresh token rejected, test logout deletes token

  **Must NOT do**:
  - Do not return the old refresh token — always rotate

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 14-15, 17-18)
  - **Blocks**: Task 18 (main.rs wiring)
  - **Blocked By**: Tasks 11 (JWT utilities), 14 (auth middleware)

  **References**:
  - `pactum_backend_spec.md` lines 782-813 — §7.5 refresh + logout exact code
  - `pactum_backend_spec.md` lines 766-773 — refresh_tokens table schema

  **WHY Each Reference Matters**:
  - §7.5: Delete-on-use rotation detects token theft — if attacker refreshes first, legitimate client fails

  **Acceptance Criteria**:
  - [ ] `POST /auth/refresh` rotates tokens correctly
  - [ ] Invalid refresh token returns `InvalidRefreshToken`
  - [ ] `POST /auth/logout` deletes the refresh token
  - [ ] `cargo test -- handlers::auth::tests::refresh` passes

  **QA Scenarios:**
  ```
  Scenario: Token refresh rotation
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::auth::tests::test_refresh`
      2. Verify new access_token + new refresh_token returned
      3. Verify old refresh_token is deleted from DB
    Expected Result: Token rotation works, old token invalidated
    Evidence: .sisyphus/evidence/task-16-refresh.txt
  ```

  **Commit**: YES (groups with Wave 3)
  - Message: `feat(auth): token refresh rotation + logout`
  - Files: `src/handlers/auth.rs`
  - Pre-commit: `cargo test -- handlers::auth`

- [ ] 17. **User Handlers: Profile + Contacts (handlers/user.rs)**

  **What to do**:
  - Implement `GET /user/me` per spec §8.5:
    - Return `{ id, display_name, linked_auth_methods: [{ type: "wallet", pubkey }, { type: "oauth", provider }] }`
    - Query user_accounts + auth_wallet + auth_oauth
  - Implement `PUT /user/me` per spec §8.5:
    - `sanitise_display_name()` per spec — reject if >64 chars or contains `< > " ' &`
    - Return `AppError::DisplayNameTooLong` or `AppError::InvalidDisplayName`
  - Implement `PUT /user/contacts` per spec §8.5:
    - Encrypt email/phone/push_token with AES-256-GCM (using crypto service from Task 9)
    - Compute HMAC blind index for email
    - UPSERT into user_contacts table
  - Implement `DELETE /user/contacts`:
    - Delete user_contacts row for the user
  - TDD: test profile returns correct structure, test display_name validation, test contact encryption

  **Must NOT do**:
  - Do not store PII in plaintext — always encrypt
  - Do not allow `<script>` or HTML in display_name

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 14-16, 18)
  - **Blocks**: Task 18 (main.rs wiring)
  - **Blocked By**: Tasks 9 (crypto service), 14 (auth middleware)

  **References**:
  - `pactum_backend_spec.md` lines 1124-1165 — §8.5 User routes + display_name validation code
  - `pactum_backend_spec.md` lines 1133-1152 — sanitise_display_name() exact implementation
  - `pactum_backend_spec.md` lines 306-322 — §5.2 user_contacts encrypted fields

  **WHY Each Reference Matters**:
  - §8.5: display_name sanitization rejects HTML chars — prevents XSS in email templates
  - §5.2: Columns are email_enc, email_nonce, email_index — must match crypto service output format

  **Acceptance Criteria**:
  - [ ] `GET /user/me` returns profile with auth methods
  - [ ] `PUT /user/me` rejects `<script>` in display_name
  - [ ] `PUT /user/contacts` encrypts email and stores HMAC index
  - [ ] `cargo test -- handlers::user` passes

  **QA Scenarios:**
  ```
  Scenario: Display name validation
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::user::tests::test_display_name_validation`
      2. Verify "Alice" is accepted, "<script>alert('xss')</script>" is rejected
      3. Verify 65-char name returns DisplayNameTooLong
    Expected Result: Validation works per spec
    Evidence: .sisyphus/evidence/task-17-display-name.txt

  Scenario: Contact encryption
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::user::tests::test_contact_upsert`
      2. Verify email is encrypted (stored bytes ≠ plaintext bytes)
      3. Verify email_index is deterministic HMAC
    Expected Result: PII encrypted in database
    Evidence: .sisyphus/evidence/task-17-contacts.txt
  ```

  **Commit**: YES (groups with Wave 3)
  - Message: `feat(user): profile, display_name validation, encrypted contacts`
  - Files: `src/handlers/user.rs`
  - Pre-commit: `cargo test -- handlers::user`

- [ ] 18. **main.rs MVP: Wire Foundation + Auth Routes + Server Startup**

  **What to do**:
  - Implement `src/main.rs` MVP that wires together foundation + auth:
    - Load `.env` via `dotenvy::dotenv()`
    - Initialize `tracing_subscriber` with env filter
    - Load Config from environment (Task 3)
    - Connect to PostgreSQL via `PgPool` (with retry logic)
    - Run `sqlx::migrate!().run(&pool)` for auto-migration on startup
    - Load vault + treasury keypairs via `load_keypair()` (Task 10)
    - Validate keypair pubkeys via `validate_keypair_pubkeys()` (Task 10)
    - Create `RpcClient` for Solana devnet
    - Build `AppState` with all components
    - Call `build_router(state)` to get the Router (Task 6)
    - Wire auth routes: challenge, verify, refresh, logout + user routes into router
    - Bind to `SERVER_HOST:SERVER_PORT` and start Axum server
    - Print "Pactum backend listening on {host}:{port}"
  - TDD: test that server starts without panic (smoke test)

  **Must NOT do**:
  - Do not wire ALL routes yet — only auth + user (MVP)
  - Do not spawn workers yet — deferred to Task 42

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`]

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 3 (sequential — last task in wave)
  - **Blocks**: Tasks 21-23 (agreement handlers depend on running server)
  - **Blocked By**: Tasks 5 (AppState), 6 (router), 10 (keypair loading), 14-17 (all auth/user handlers)

  **References**:
  - `pactum_backend_spec.md` lines 557-588 — §6 router.rs build_router code
  - `pactum_backend_spec.md` lines 228-262 — §4 AppState definition
  - `pactum_backend_spec.md` lines 224-226 — SERVER_PORT=8080, SERVER_HOST=0.0.0.0

  **WHY Each Reference Matters**:
  - §6: Router merges route groups — only auth + user initially, expand in Task 42
  - §4: AppState construction order matters — DB pool must be ready before creating state

  **Acceptance Criteria**:
  - [ ] `cargo build` succeeds
  - [ ] Server starts with mock/test config and binds to port
  - [ ] `GET /auth/challenge` returns a nonce (smoke test)
  - [ ] `cargo test -- main` passes

  **QA Scenarios:**
  ```
  Scenario: Server starts successfully
    Tool: Bash
    Preconditions: PostgreSQL running, .env configured
    Steps:
      1. Run `cargo build`
      2. Start server in background: `./target/debug/pactum-backend &`
      3. Wait 3 seconds for startup
      4. Run `curl -s http://localhost:8080/auth/challenge`
      5. Verify response contains `nonce` field
      6. Kill background server process
    Expected Result: Server starts, challenge endpoint responds
    Failure Indicators: Panic at startup, connection refused
    Evidence: .sisyphus/evidence/task-18-server-start.txt
  ```

  **Commit**: YES (groups with Wave 3)
  - Message: `feat: main.rs MVP — server startup with auth routes`
  - Files: `src/main.rs`
  - Pre-commit: `cargo build`


- [ ] 19. **create_agreement TX Construction (services/solana.rs - create)**

  **What to do**:
  - Implement `build_create_agreement_tx()` in `services/solana.rs` per spec §11.3:
    - Derive agreement PDA, mint_vault PDA, pda_authority PDA
    - Calculate vault_deposit = `getMinimumBalanceForRentExemption(AGREEMENT_STATE_SIZE) + VAULT_BUFFER`
    - Build system_instruction::transfer from vault_keypair to mint_vault_pda
    - Build create_agreement instruction with Anchor discriminator + borsh-serialized `CreateAgreementArgs`
    - Account ordering: creator, agreement PDA, system_program, mint_vault PDA, vault_keypair, MPL Core program, pda_authority, collection, etc. (match on-chain instruction exactly)
    - Validate vault deposit does not exceed max (`validate_create_agreement_tx()`)
    - Assemble Transaction, partial_sign with vault_keypair, serialize to base64
  - TDD: test TX construction produces valid structure, test vault deposit validation rejects excessive amount

  **Must NOT do**:
  - Do not fully sign — only partial_sign with vault_keypair (creator signs client-side)
  - Do not use anchor-client

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Tasks 20-23)
  - **Blocks**: Task 21 (POST /agreement handler)
  - **Blocked By**: Task 12 (Solana service foundation)

  **References**:
  - `pactum_backend_spec.md` lines 1752-1808 — §11.3 exact build_create_agreement_tx + validate code
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/create_agreement.rs` — Account ordering
  - Draft notes: PDA seeds, discriminator for "create_agreement"

  **Acceptance Criteria**:
  - [ ] TX includes vault transfer + create_agreement instruction
  - [ ] vault_keypair partial-signs the TX
  - [ ] Excessive vault deposit rejected by validation
  - [ ] `cargo test -- services::solana::tests::create_agreement` passes

  **QA Scenarios:**
  ```
  Scenario: Create agreement TX structure
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::solana::tests::test_build_create_tx`
      2. Verify TX has exactly 2 instructions (transfer + create)
      3. Verify vault_keypair signature is present
    Expected Result: Valid partially-signed transaction
    Evidence: .sisyphus/evidence/task-19-create-tx.txt
  ```

  **Commit**: YES (groups with Wave 4)
  - Message: `feat(solana): create_agreement TX construction + validation`
  - Files: `src/services/solana.rs`
  - Pre-commit: `cargo test -- services::solana`

- [ ] 20. **sign_agreement TX Construction (services/solana.rs - sign)**

  **What to do**:
  - Implement `build_sign_agreement_tx()` in `services/solana.rs`:
    - Derive agreement PDA from creator + agreement_id
    - Build sign_agreement instruction with Anchor discriminator + borsh-serialized `SignAgreementArgs { metadata_uri }`
    - Account ordering: signer (party), agreement PDA, system_program, collection, pda_authority, MPL Core program (for NFT minting on final sign)
    - If this is the final signature: `metadata_uri` must be Some (generated in metadata service)
    - This TX is **not** partially signed by platform — returned unsigned for party to sign
  - Also implement stubs for: `build_cancel_agreement_tx()`, `build_expire_agreement_tx()`, `build_vote_revoke_tx()`, `build_retract_revoke_vote_tx()`, `build_close_agreement_tx()`
  - TDD: test sign TX structure, test unsigned TX has no signatures

  **Must NOT do**:
  - Do not sign on behalf of users

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Tasks 19, 21-23)
  - **Blocks**: Task 22 (POST /agreement/{pda}/sign handler)
  - **Blocked By**: Task 12 (Solana service foundation)

  **References**:
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/sign_agreement.rs` — Account ordering for sign
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/cancel_agreement.rs` — cancel accounts
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/expire_agreement.rs` — expire accounts
  - `pactum_backend_spec.md` lines 1420-1428 — §9.4 sign response includes suggest_email flag

  **Acceptance Criteria**:
  - [ ] sign_agreement TX includes correct instruction + accounts
  - [ ] TX is returned unsigned (no signatures)
  - [ ] Stubs for cancel/expire/revoke/retract/close compile
  - [ ] `cargo test -- services::solana::tests::sign_agreement` passes

  **QA Scenarios:**
  ```
  Scenario: Sign agreement TX is unsigned
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::solana::tests::test_build_sign_tx`
      2. Verify TX has 0 signatures (party signs client-side)
      3. Verify sign_agreement instruction has correct discriminator
    Expected Result: Valid unsigned transaction
    Evidence: .sisyphus/evidence/task-20-sign-tx.txt
  ```

  **Commit**: YES (groups with Wave 4)
  - Message: `feat(solana): sign/cancel/expire/revoke TX construction`
  - Files: `src/services/solana.rs`
  - Pre-commit: `cargo test -- services::solana`

- [ ] 21. **POST /agreement Handler (handlers/agreement.rs - create)**

  **What to do**:
  - Implement `POST /agreement` handler per spec §8.3 + §8.4:
    - Auth: `AuthUserWithWallet` (requires wallet)
    - Request body: `{ title, parties: [{ pubkey } | { email }], expires_in_secs }`
    - Validate: `INVITE_EXPIRY_SECONDS < expires_in_secs` (return InviteWindowExceedsSigningWindow)
    - Party resolution flow (spec §8.4):
      - For each party: if pubkey provided, use directly. If email, compute HMAC blind index, lookup in user_contacts
      - FOUND + has wallet → resolve immediately
      - FOUND + no wallet OR NOT FOUND → create party_invitations row, send invitation email
    - If all resolved: build create_agreement TX, return `{ status: "submitted", transaction, agreement_pda }`
    - If not all resolved: create agreement_drafts row, return `{ status: "awaiting_party_wallets", draft_id, pending_invitations }`
    - Insert agreement_parties rows for resolved parties
  - Check free tier via `resolve_payment_requirement()` — if paid tier required, enforce payment before submit
  - TDD: test all-resolved path returns TX, test partial-resolved path returns draft

  **Must NOT do**:
  - Do not store email in draft_payload — PII isolation per L-4
  - Do not skip INVITE_EXPIRY validation

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Tasks 19-20, 22-23)
  - **Blocks**: Task 42 (final wiring)
  - **Blocked By**: Tasks 14 (auth), 18 (main.rs), 19 (create TX)

  **References**:
  - `pactum_backend_spec.md` lines 931-1104 — §8.4 POST /agreement full resolution flow
  - `pactum_backend_spec.md` lines 976-982 — InviteWindowExceedsSigningWindow validation
  - `pactum_backend_spec.md` lines 368-392 — §5.5 agreement_drafts table
  - `pactum_backend_spec.md` lines 326-343 — §5.3 agreement_parties table

  **Acceptance Criteria**:
  - [ ] All-resolved parties path returns partially-signed TX
  - [ ] Partial-resolved path creates draft + invitations
  - [ ] INVITE_EXPIRY validation works
  - [ ] `cargo test -- handlers::agreement::tests::create` passes

  **QA Scenarios:**
  ```
  Scenario: All parties resolved → TX returned
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::agreement::tests::test_create_all_resolved`
      2. Verify response has status="submitted" and transaction field
    Expected Result: TX returned for signing
    Evidence: .sisyphus/evidence/task-21-create-resolved.txt

  Scenario: Unresolved parties → draft created
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::agreement::tests::test_create_unresolved`
      2. Verify response has status="awaiting_party_wallets" and draft_id
    Expected Result: Draft created with pending invitations
    Evidence: .sisyphus/evidence/task-21-create-draft.txt
  ```

  **Commit**: YES (groups with Wave 4)
  - Message: `feat(agreement): POST /agreement with party resolution + draft creation`
  - Files: `src/handlers/agreement.rs`
  - Pre-commit: `cargo test -- handlers::agreement`

- [ ] 22. **POST /agreement/{pda}/sign Handler (handlers/agreement.rs - sign)**

  **What to do**:
  - Implement `POST /agreement/{pda}/sign` handler per spec §8.3:
    - Auth: `AuthUserWithWallet`
    - Fetch agreement from chain (or DB cache) to verify party is authorized
    - If final signature: require metadata_uri (use metadata service from Task 28)
    - Build sign_agreement TX (unsigned) using Task 20
    - Return `{ transaction, suggest_email: bool, suggest_email_reason }` per spec §9.4
    - `suggest_email = true` if user has no email on file (SIWS users who skipped)
  - TDD: test sign returns unsigned TX, test suggest_email flag

  **Must NOT do**:
  - Do not sign the TX — party signs client-side

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Tasks 19-21, 23)
  - **Blocks**: Task 42 (final wiring)
  - **Blocked By**: Tasks 14 (auth), 20 (sign TX), 28 (metadata — soft dep, can stub)

  **References**:
  - `pactum_backend_spec.md` lines 1420-1428 — §9.4 sign response with suggest_email
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/sign_agreement.rs` — Sign accounts

  **Acceptance Criteria**:
  - [ ] Sign handler returns unsigned TX
  - [ ] suggest_email is true when user has no email
  - [ ] `cargo test -- handlers::agreement::tests::sign` passes

  **QA Scenarios:**
  ```
  Scenario: Sign agreement handler
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::agreement::tests::test_sign`
      2. Verify response contains transaction field (unsigned)
      3. Verify suggest_email flag present for SIWS users without email
    Expected Result: Unsigned TX + suggest_email flag
    Evidence: .sisyphus/evidence/task-22-sign.txt
  ```

  **Commit**: YES (groups with Wave 4)
  - Message: `feat(agreement): POST /agreement/{pda}/sign handler`
  - Files: `src/handlers/agreement.rs`
  - Pre-commit: `cargo test -- handlers::agreement`

- [ ] 23. **GET /agreement/{pda} + GET /agreements (handlers/agreement.rs - read)**

  **What to do**:
  - Implement `GET /agreement/{pda}` per spec §8.3:
    - Public endpoint (no auth required)
    - Fetch agreement state from Solana chain via RPC `getAccountInfo`
    - Deserialize Anchor account data (skip 8-byte discriminator, then borsh deserialize)
    - Return agreement fields: status, parties, signed_by, created_at, expires_at, title, etc.
  - Implement `GET /agreements` per spec §8.3:
    - Auth: `AuthUser` (JWT)
    - Query `agreement_parties` table WHERE `party_pubkey = auth.pubkey` or role filter
    - Support query params: `status`, `role` (creator|party|any), `page`, `limit` (default 20)
    - Return paginated list
  - TDD: test single agreement fetch, test list with pagination, test status filter

  **Must NOT do**:
  - Do not expose private keys or internal state

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Tasks 19-22)
  - **Blocks**: Task 42 (final wiring)
  - **Blocked By**: Tasks 14 (auth), 18 (main.rs)

  **References**:
  - `pactum_backend_spec.md` lines 1116-1122 — GET /agreements query params
  - `pactum_backend_spec.md` lines 326-343 — §5.3 agreement_parties table

  **Acceptance Criteria**:
  - [ ] `GET /agreement/{pda}` returns on-chain agreement state
  - [ ] `GET /agreements` returns paginated list filtered by status/role
  - [ ] `cargo test -- handlers::agreement::tests::read` passes

  **QA Scenarios:**
  ```
  Scenario: Agreement list with filters
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::agreement::tests::test_list_agreements`
      2. Verify pagination works (page=1, limit=20)
      3. Verify status filter returns only matching agreements
    Expected Result: Paginated, filtered results
    Evidence: .sisyphus/evidence/task-23-list.txt
  ```

  **Commit**: YES (groups with Wave 4)
  - Message: `feat(agreement): GET /agreement/{pda} + GET /agreements with pagination`
  - Files: `src/handlers/agreement.rs`
  - Pre-commit: `cargo test -- handlers::agreement`


- [ ] 24. **OAuth2 Google + Microsoft (handlers/auth.rs - OAuth)**

  **What to do**:
  - Implement `GET /auth/oauth/google` per spec §7.2: redirect to Google consent screen with CSRF state parameter
  - Implement `GET /auth/oauth/google/callback`: exchange code for provider access token, fetch user profile (sub, email), check cross-provider email conflict (M-5), upsert user_accounts + auth_oauth, return JWT tokens
  - Implement `GET /auth/oauth/microsoft` + callback: same flow with Microsoft tenant=common
  - Store CSRF state in session/cookie for validation on callback
  - Handle `409 EmailAlreadyRegistered` with `{ existing_provider, link_url }` per M-5 fix
  - Encrypt + store email in user_contacts on OAuth signup
  - TDD: test redirect URL generation, test callback token exchange (mocked HTTP), test email conflict detection

  **Must NOT do**:
  - Do not implement Apple OAuth (deferred)
  - Do not silently merge accounts on email conflict

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5 (with Tasks 25-29)
  - **Blocks**: Task 42 (final wiring)
  - **Blocked By**: Tasks 11 (JWT), 14 (auth middleware)

  **References**:
  - `pactum_backend_spec.md` lines 648-676 — §7.2 OAuth2 flow, callback logic, M-5 email conflict
  - `pactum_backend_spec.md` lines 158-165 — Google/Microsoft OAuth env vars

  **Acceptance Criteria**:
  - [ ] Google OAuth redirect + callback works with mocked provider
  - [ ] Microsoft OAuth redirect + callback works
  - [ ] Email conflict returns 409 with existing_provider
  - [ ] `cargo test -- handlers::auth::tests::oauth` passes

  **QA Scenarios:**
  ```
  Scenario: OAuth redirect URL generation
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::auth::tests::test_google_redirect`
      2. Verify redirect URL contains client_id, redirect_uri, scope, state
    Expected Result: Valid Google OAuth redirect URL
    Evidence: .sisyphus/evidence/task-24-oauth-redirect.txt

  Scenario: Email conflict detection
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::auth::tests::test_email_conflict`
      2. Create user via Google, attempt Microsoft with same email
      3. Verify 409 EmailAlreadyRegistered with existing_provider="google"
    Expected Result: Conflict detected, not silently merged
    Evidence: .sisyphus/evidence/task-24-email-conflict.txt
  ```

  **Commit**: YES (groups with Wave 5)
  - Message: `feat(auth): OAuth2 Google + Microsoft with email conflict detection`
  - Files: `src/handlers/auth.rs`
  - Pre-commit: `cargo test -- handlers::auth`

- [ ] 25. **POST /auth/link/wallet Handler**

  **What to do**:
  - Implement `POST /auth/link/wallet` per spec §7.3:
    - Auth: `AuthUser` (JWT, OAuth user)
    - Body: `{ pubkey, signature, nonce }`
    - Consume nonce atomically (same as SIWS verify)
    - Verify ed25519 signature
    - INSERT into auth_wallet (or return error if pubkey already linked)
    - Issue new JWT with pubkey included in claims
  - TDD: test link creates auth_wallet row, test new JWT includes pubkey

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 14 (auth), 15 (SIWS nonce logic)

  **References**:
  - `pactum_backend_spec.md` lines 677-706 — §7.3 link wallet flow

  **Acceptance Criteria**:
  - [ ] Wallet linked, new JWT includes pubkey
  - [ ] `cargo test -- handlers::auth::tests::link_wallet` passes

  **QA Scenarios:**
  ```
  Scenario: Link wallet to OAuth account
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::auth::tests::test_link_wallet`
      2. Verify auth_wallet row created
      3. Verify new JWT has pubkey field populated
    Expected Result: Wallet linked, JWT updated
    Evidence: .sisyphus/evidence/task-25-link-wallet.txt
  ```

  **Commit**: YES (groups with Wave 5)
  - Message: `feat(auth): POST /auth/link/wallet for OAuth users`
  - Files: `src/handlers/auth.rs`

- [ ] 26. **Upload Handler: Multipart + Hash Verify (handlers/upload.rs)**

  **What to do**:
  - Implement `POST /upload` per spec §8.2:
    - Auth: `AuthUserWithWallet`
    - Parse multipart: file binary, client_hash (hex), backend ("ipfs" | "arweave")
    - Validate content type against `ALLOWED_MIME_TYPES` (pdf, png, jpeg)
    - Enforce `MAX_FILE_SIZE_BYTES` (50MB) via `with_size_limit()`
    - Compute SHA-256 using hash service (Task 8)
    - Verify client_hash matches server hash (return HashMismatch on mismatch)
    - Upload to IPFS/Arweave via storage service (Task 27)
    - Return `{ storage_uri, content_hash, storage_backend }`
  - Rate limit: 10 req/min per IP
  - TDD: test valid upload, test hash mismatch, test invalid file type, test file too large

  **Must NOT do**:
  - Do not accept file types outside ALLOWED_MIME_TYPES
  - Do not skip hash verification

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5
  - **Blocks**: Task 30 (draft submit needs upload)
  - **Blocked By**: Tasks 8 (hash service), 14 (auth), 27 (storage service)

  **References**:
  - `pactum_backend_spec.md` lines 833-905 — §8.2 upload handler with validation code
  - `pactum_backend_spec.md` lines 854-885 — Exact upload_handler implementation

  **Acceptance Criteria**:
  - [ ] Valid PDF upload returns storage_uri + content_hash
  - [ ] Hash mismatch returns 400
  - [ ] Invalid file type returns error
  - [ ] `cargo test -- handlers::upload` passes

  **QA Scenarios:**
  ```
  Scenario: Upload with hash verification
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::upload::tests::test_valid_upload`
      2. Verify storage_uri returned (mocked storage backend)
      3. Run `cargo test -- handlers::upload::tests::test_hash_mismatch`
      4. Verify HashMismatch error returned
    Expected Result: Upload validation works
    Evidence: .sisyphus/evidence/task-26-upload.txt
  ```

  **Commit**: YES (groups with Wave 5)
  - Message: `feat(upload): multipart upload with hash verify + size/type validation`
  - Files: `src/handlers/upload.rs`

- [ ] 27. **Storage Service: IPFS + Arweave Upload (services/storage.rs)**

  **What to do**:
  - Create `src/services/storage.rs`:
    - `upload_to_ipfs(data: &[u8], config: &Config) -> Result<String, AppError>` — POST to Pinata API, return `ipfs://Qm...` URI
    - `upload_to_arweave(data: &[u8], config: &Config) -> Result<String, AppError>` — POST to Arweave, return `ar://...` URI
    - `upload_document(backend: &str, data: &[u8], config: &Config) -> Result<String, AppError>` — dispatch to IPFS or Arweave
  - Use `reqwest` for HTTP calls to storage providers
  - TDD: test dispatch to correct backend, test error handling (mocked HTTP)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5
  - **Blocks**: Tasks 26 (upload handler), 30 (draft submit)
  - **Blocked By**: Task 1 (scaffolding)

  **References**:
  - `pactum_backend_spec.md` lines 220-222 — IPFS_API_URL, IPFS_JWT, ARWEAVE_WALLET_PATH

  **Acceptance Criteria**:
  - [ ] IPFS upload returns `ipfs://...` URI
  - [ ] Arweave upload returns `ar://...` URI
  - [ ] `cargo test -- services::storage` passes

  **QA Scenarios:**
  ```
  Scenario: Storage backend dispatch
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::storage::tests`
      2. Verify "ipfs" dispatches to upload_to_ipfs
      3. Verify "arweave" dispatches to upload_to_arweave
    Expected Result: Correct backend selected
    Evidence: .sisyphus/evidence/task-27-storage.txt
  ```

  **Commit**: YES (groups with Wave 5)
  - Message: `feat(storage): IPFS + Arweave upload service`
  - Files: `src/services/storage.rs`

- [ ] 28. **Metadata Generation: NFT Metadata JSON (services/metadata.rs)**

  **What to do**:
  - Create `src/services/metadata.rs` per spec §11.4:
    - `build_metadata_json(agreement: &AgreementState) -> serde_json::Value`
    - Include: name, description, image (Pactum seal), animation_url, external_url, attributes
    - Attributes: agreement_id, content_hash, parties, signed_at, storage_uri
  - After generation, upload JSON to IPFS/Arweave via storage service
  - TDD: test metadata JSON structure matches expected format

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5
  - **Blocks**: Task 22 (sign handler needs metadata_uri for final sign)
  - **Blocked By**: Task 7 (solana types)

  **References**:
  - `pactum_backend_spec.md` lines 1859-1877 — §11.4 build_metadata_json exact code

  **Acceptance Criteria**:
  - [ ] Metadata JSON has required fields: name, description, image, attributes
  - [ ] `cargo test -- services::metadata` passes

  **QA Scenarios:**
  ```
  Scenario: Metadata JSON structure
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::metadata::tests`
      2. Verify JSON contains name with "Pactum #" prefix
      3. Verify attributes include content_hash and parties
    Expected Result: Valid NFT metadata JSON
    Evidence: .sisyphus/evidence/task-28-metadata.txt
  ```

  **Commit**: YES (groups with Wave 5)
  - Message: `feat(metadata): NFT metadata JSON generation`
  - Files: `src/services/metadata.rs`

- [ ] 29. **WebSocket Handler: Upgrade + Per-User Channels (handlers/ws.rs)**

  **What to do**:
  - Implement `GET /ws` handler per spec §10:
    - Auth: JWT via query param or header
    - Validate Origin header against allowlist (L-1 fix)
    - WebSocket upgrade via `WebSocketUpgrade`
    - On connect: create `broadcast::channel(64)`, insert sender into `ws_channels` DashMap keyed by user_id
    - On message from server: serialize WsEvent to JSON, send to client
    - On disconnect: remove from `ws_channels` DashMap
  - Implement `send_to_user()` and `send_to_users()` helper functions per spec §10.3
  - TDD: test WS upgrade, test event delivery, test disconnect cleanup

  **Must NOT do**:
  - Do not implement multi-session fan-out (v0.1 = single session per user)
  - Do not skip Origin validation

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 5
  - **Blocks**: Task 37 (event listener broadcasts WS events)
  - **Blocked By**: Tasks 5 (AppState with ws_channels), 14 (auth)

  **References**:
  - `pactum_backend_spec.md` lines 1592-1692 — §10 WebSocket full implementation
  - `pactum_backend_spec.md` lines 1633-1691 — §10.3 broadcast architecture + Origin validation

  **Acceptance Criteria**:
  - [ ] WS upgrade works with valid JWT
  - [ ] Events delivered to connected client
  - [ ] Disconnect removes channel from DashMap
  - [ ] Origin validation rejects invalid origins
  - [ ] `cargo test -- handlers::ws` passes

  **QA Scenarios:**
  ```
  Scenario: WebSocket connection lifecycle
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::ws::tests`
      2. Verify WS upgrade succeeds with valid JWT
      3. Verify event sent via send_to_user() is received
      4. Verify disconnect removes user from ws_channels
    Expected Result: Full WS lifecycle works
    Evidence: .sisyphus/evidence/task-29-ws.txt
  ```

  **Commit**: YES (groups with Wave 5)
  - Message: `feat(ws): WebSocket upgrade + per-user channels + Origin validation`
  - Files: `src/handlers/ws.rs`

- [ ] 30. **Draft Handlers: GET/DELETE/PUT/POST /draft/* (handlers/draft.rs)**

  **What to do**:
  - Implement `GET /draft/{id}` per spec §8.4: return draft status, party_slots, pending invitations
  - Implement `DELETE /draft/{id}`: creator only, set status='discarded', no on-chain action
  - Implement `PUT /draft/{id}/reinvite`: creator resends invitation to expired party slot, create new party_invitations row with fresh token
  - Implement `POST /draft/{id}/submit` per spec §9.4:
    - Gate 1: `draft.paid == true` (return PaymentRequired with initiate_url)
    - Gate 2: `draft.status == ready_to_submit` (return DraftNotReady)
    - Gate 3: creator has email on file (return EmailRequired)
    - Upload document to Arweave/IPFS
    - Set `storage_uploaded = true` atomically (point of no return for refund)
    - Build create_agreement TX, return to creator
  - TDD: test each handler, test payment gate, test ready gate, test email gate

  **Must NOT do**:
  - Do not allow non-creator to delete/reinvite
  - Do not skip the 3-gate validation sequence

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6 (with Tasks 31-36)
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 9 (crypto), 14 (auth), 27 (storage)

  **References**:
  - `pactum_backend_spec.md` lines 920-930 — §8.4 draft routes table
  - `pactum_backend_spec.md` lines 1361-1418 — §9.4 submit_draft exact code with 3 gates
  - `pactum_backend_spec.md` lines 368-392 — §5.5 agreement_drafts table

  **Acceptance Criteria**:
  - [ ] GET returns draft status and party slots
  - [ ] DELETE sets status='discarded' (creator only)
  - [ ] POST /submit enforces payment→ready→email gates in sequence
  - [ ] `cargo test -- handlers::draft` passes

  **QA Scenarios:**
  ```
  Scenario: Draft submit 3-gate validation
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::draft::tests::test_submit_unpaid`
      2. Verify PaymentRequired returned with initiate_url
      3. Run `cargo test -- handlers::draft::tests::test_submit_not_ready`
      4. Verify DraftNotReady returned
    Expected Result: Gates enforced in correct order
    Evidence: .sisyphus/evidence/task-30-draft-gates.txt
  ```

  **Commit**: YES (groups with Wave 6)
  - Message: `feat(draft): GET/DELETE/PUT/POST draft handlers with 3-gate submit`
  - Files: `src/handlers/draft.rs`

- [ ] 31. **Invitation Handlers: GET/POST /invite/* (handlers/invite.rs)**

  **What to do**:
  - Implement `GET /invite/{token}` per spec §5.6 + §8.5:
    - Public endpoint (no auth), rate limited 5 req/min per IP
    - Look up invitation by token, verify not expired
    - Return `{ agreement_title, creator_display, expires_at, has_account, has_wallet }` — never return full email (M-6)
  - Implement `POST /invite/{token}/accept` per spec §8.5:
    - Auth: `AuthUserWithWallet`
    - Mark invitation status = 'accepted'
    - Store resolved pubkey in party_slots of agreement_drafts
    - Check if ALL party slots resolved → if yes, mark draft status='ready_to_submit', notify creator (WS + email)
  - TDD: test token validation, test accept resolves pubkey, test all-resolved triggers notification

  **Must NOT do**:
  - Do not return full email in GET response — masked only

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 9 (crypto for email decrypt), 14 (auth)

  **References**:
  - `pactum_backend_spec.md` lines 1000-1044 — §8.5 accept flow diagram
  - `pactum_backend_spec.md` lines 455-465 — GET /invite/{token} response format + rate limit
  - `pactum_backend_spec.md` lines 983-998 — has_account/has_wallet frontend logic

  **Acceptance Criteria**:
  - [ ] GET /invite/{token} returns masked preview (no full email)
  - [ ] POST /invite/{token}/accept marks accepted + resolves pubkey
  - [ ] All-resolved triggers draft ready_to_submit + creator notification
  - [ ] `cargo test -- handlers::invite` passes

  **QA Scenarios:**
  ```
  Scenario: Invitation accept triggers draft resolution
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::invite::tests::test_accept_resolves_draft`
      2. Create draft with 2 parties (1 resolved, 1 pending)
      3. Accept pending invitation
      4. Verify draft status changed to ready_to_submit
    Expected Result: Draft auto-resolves when all parties accept
    Evidence: .sisyphus/evidence/task-31-invite-accept.txt
  ```

  **Commit**: YES (groups with Wave 6)
  - Message: `feat(invite): GET/POST invitation handlers with draft resolution`
  - Files: `src/handlers/invite.rs`

- [ ] 32. **Stablecoin Registry + Solana Pay Service (services/solana_pay.rs)**

  **What to do**:
  - Create `src/services/solana_pay.rs` per spec §9.3:
    - `StablecoinInfo` struct with symbol, mint, ata, decimals (always 6)
    - `StablecoinRegistry` with usdc, usdt, pyusd + `resolve(&self, method: &str)` method
    - Build StablecoinRegistry from config env vars at startup
    - `generate_payment_reference() -> Keypair` — unique reference keypair for Solana Pay tx identification
    - `build_solana_pay_url(ata, amount, mint, reference, label, memo) -> String`
    - `confirm_payment_atomic(db, reference, tx_sig, token_mint, token_amount) -> Result<bool, AppError>` — atomic UPDATE WHERE status='pending' RETURNING id
    - Payment polling: `poll_payment_confirmation(rpc, reference) -> Option<(String, String, i64)>` — check getSignaturesForAddress, verify mint + amount
  - TDD: test registry resolve, test atomic confirmation idempotency, test Solana Pay URL format

  **Must NOT do**:
  - Do not accept payment without verifying token_mint matches expected mint

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6
  - **Blocks**: Tasks 33 (payment handler), 39 (refund worker)
  - **Blocked By**: Tasks 5 (AppState), 12 (Solana service)

  **References**:
  - `pactum_backend_spec.md` lines 1215-1245 — §9.3 StablecoinInfo + Registry code
  - `pactum_backend_spec.md` lines 1256-1338 — Payment flow + confirm_payment_atomic code
  - `pactum_backend_spec.md` lines 1340-1347 — payment_tx_sig_unique index

  **Acceptance Criteria**:
  - [ ] Registry resolves "usdc"/"usdt"/"pyusd" to correct StablecoinInfo
  - [ ] Atomic confirmation prevents double-confirmation
  - [ ] Solana Pay URL is correctly formatted
  - [ ] `cargo test -- services::solana_pay` passes

  **QA Scenarios:**
  ```
  Scenario: Atomic payment confirmation
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::solana_pay::tests::test_atomic_confirm`
      2. First confirm returns true (pending→confirmed)
      3. Second confirm returns false (already confirmed — idempotent)
    Expected Result: No double confirmation
    Evidence: .sisyphus/evidence/task-32-atomic-confirm.txt
  ```

  **Commit**: YES (groups with Wave 6)
  - Message: `feat(solana_pay): stablecoin registry + atomic payment confirmation`
  - Files: `src/services/solana_pay.rs`

- [ ] 33. **Payment Handlers: Initiate + Status (handlers/payment.rs)**

  **What to do**:
  - Implement `POST /payment/initiate/{draft_id}` per spec §9.1:
    - Auth: `AuthUser` (JWT)
    - Body: `{ method: "usdc" | "usdt" | "pyusd" }`
    - Check free tier via resolve_payment_requirement()
    - Resolve method to StablecoinInfo via registry
    - Generate unique reference keypair
    - Create agreement_payments row with status='pending'
    - Return `{ method, token_mint, treasury_ata, amount_units, reference_pubkey, solana_pay_url }`
    - Start background polling for payment confirmation
  - Implement `GET /payment/status/{draft_id}`:
    - Return current payment status (pending/confirmed/failed)
  - TDD: test free tier bypass, test payment initiation, test unknown method rejected

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 5 (AppState), 32 (Solana Pay service)

  **References**:
  - `pactum_backend_spec.md` lines 1182-1210 — §9.1-§9.2 payment routes + free tier check
  - `pactum_backend_spec.md` lines 1256-1298 — Payment flow diagram

  **Acceptance Criteria**:
  - [ ] Free tier users bypass payment
  - [ ] Paid tier users get Solana Pay URL
  - [ ] Unknown method returns PaymentMethodUnsupported
  - [ ] `cargo test -- handlers::payment` passes

  **QA Scenarios:**
  ```
  Scenario: Free tier bypass
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::payment::tests::test_free_tier`
      2. User with free_used < 3 should not require payment
    Expected Result: Payment skipped for free tier
    Evidence: .sisyphus/evidence/task-33-free-tier.txt
  ```

  **Commit**: YES (groups with Wave 6)
  - Message: `feat(payment): initiate + status handlers with free tier`
  - Files: `src/handlers/payment.rs`

- [ ] 34. **Refund Service: Calculate + Execute (services/refund.rs)**

  **What to do**:
  - Create `src/services/refund.rs` per spec §9.5:
    - `calculate_refund_amount(paid_units, nonrefundable_cents, total_fee_cents) -> u64` — formula: `paid_units * (total - nonrefundable) / total`
    - `execute_refund(rpc, treasury_keypair, payment, config) -> Result<String, AppError>` per spec §11.3:
      - Derive creator ATA from stored pubkey + token_mint
      - Verify treasury ATA matches expected (H-5 fix — compare ATA to ATA, not mint to ATA)
      - Build SPL token transfer instruction
      - Sign + submit with treasury_keypair
    - Parameters always sourced from database, never from request
  - TDD: test refund calculation (full=$1.99, partial=$1.89), test ATA validation

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6
  - **Blocks**: Task 39 (refund worker)
  - **Blocked By**: Tasks 5 (AppState), 12 (Solana service)

  **References**:
  - `pactum_backend_spec.md` lines 1442-1490 — §9.5 calculate_refund_amount + execute_refund code
  - `pactum_backend_spec.md` lines 1810-1857 — §11.3 refund transaction validation (H-5 fix)
  - `pactum_backend_spec.md` lines 1429-1441 — Refund policy table

  **Acceptance Criteria**:
  - [ ] calculate_refund_amount(1_990_000, 10, 199) = 1_890_000
  - [ ] Full refund when storage_uploaded=false = 1_990_000
  - [ ] Treasury ATA validation catches mismatches
  - [ ] `cargo test -- services::refund` passes

  **QA Scenarios:**
  ```
  Scenario: Refund calculation
    Tool: Bash
    Steps:
      1. Run `cargo test -- services::refund::tests::test_calculate`
      2. Verify full refund: 1_990_000 * (199-0)/199 = 1_990_000
      3. Verify partial refund: 1_990_000 * (199-10)/199 = 1_890_000 (integer math)
    Expected Result: Correct refund amounts
    Evidence: .sisyphus/evidence/task-34-refund-calc.txt
  ```

  **Commit**: YES (groups with Wave 6)
  - Message: `feat(refund): refund calculation + SPL transfer execution`
  - Files: `src/services/refund.rs`

- [ ] 35. **Cancel/Expire Agreement Handlers + TX Construction**

  **What to do**:
  - Implement `POST /agreement/{pda}/cancel` per spec §8.3:
    - Auth: `AuthUserWithWallet` (creator only)
    - Build cancel_agreement TX (stub from Task 20 → full implementation)
    - Return unsigned TX for creator to sign
  - Implement `POST /agreement/{pda}/expire` per spec §8.3:
    - Public endpoint (anyone can expire a stale agreement)
    - Build expire_agreement TX, partial_sign with vault_keypair
    - Return partially-signed TX
  - Complete TX construction for cancel + expire instructions in services/solana.rs
  - TDD: test cancel TX structure, test expire TX includes vault_keypair signature

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6
  - **Blocks**: Tasks 37 (event listener handles cancel/expire events), 42
  - **Blocked By**: Tasks 12 (Solana service), 14 (auth)

  **References**:
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/cancel_agreement.rs` — Cancel accounts
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/expire_agreement.rs` — Expire accounts

  **Acceptance Criteria**:
  - [ ] Cancel TX is unsigned (creator signs)
  - [ ] Expire TX is partially signed by vault_keypair
  - [ ] `cargo test -- handlers::agreement::tests::cancel_expire` passes

  **QA Scenarios:**
  ```
  Scenario: Cancel and expire TX construction
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::agreement::tests::test_cancel_tx`
      2. Verify cancel TX has correct instruction discriminator
      3. Run `cargo test -- handlers::agreement::tests::test_expire_tx`
      4. Verify expire TX has vault_keypair signature
    Expected Result: Both TX types constructed correctly
    Evidence: .sisyphus/evidence/task-35-cancel-expire.txt
  ```

  **Commit**: YES (groups with Wave 6)
  - Message: `feat(agreement): cancel + expire handlers with TX construction`
  - Files: `src/handlers/agreement.rs`, `src/services/solana.rs`

- [ ] 36. **Vote Revoke / Retract / Close Agreement Handlers + TX Construction**

  **What to do**:
  - Implement `POST /agreement/{pda}/revoke` — build vote_revoke TX (unsigned)
  - Implement `POST /agreement/{pda}/retract` — build retract_revoke_vote TX (unsigned)
  - Implement `DELETE /agreement/{pda}` — build close_agreement TX (unsigned, creator only)
  - Complete TX construction in services/solana.rs for these 3 instructions
  - TDD: test each TX structure

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 6
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 12 (Solana service), 14 (auth)

  **References**:
  - GitHub `PinkPlants/pactum/programs/pactum/src/instructions/` — vote_revoke, retract, close accounts

  **Acceptance Criteria**:
  - [ ] All 3 TX types construct correctly
  - [ ] `cargo test -- handlers::agreement::tests::revoke_retract_close` passes

  **QA Scenarios:**
  ```
  Scenario: Revoke/retract/close TX construction
    Tool: Bash
    Steps:
      1. Run `cargo test -- handlers::agreement::tests::test_revoke_tx`
      2. Verify all 3 instruction discriminators are correct
    Expected Result: All TX types valid
    Evidence: .sisyphus/evidence/task-36-revoke-close.txt
  ```

  **Commit**: YES (groups with Wave 6)
  - Message: `feat(agreement): vote_revoke, retract, close handlers`
  - Files: `src/handlers/agreement.rs`, `src/services/solana.rs`

- [ ] 37. **Event Listener Worker (workers/event_listener.rs)**

  **What to do**:
  - Create `src/workers/event_listener.rs` per spec §12.1:
    - Subscribe to Solana program logs via PubsubClient (WebSocket)
    - Parse Anchor instruction logs to identify confirmed instructions
    - `handle_confirmed_tx()` dispatch: CreateAgreement, SignAgreement, CancelAgreement, ExpireAgreement, VoteRevoke
    - For each event: update DB (agreement_parties status), enqueue notifications, broadcast WS event
    - For Cancel/Expire: also call `initiate_refund_if_eligible()` per spec §9.5
    - Auto-reconnect on disconnect (5s delay)
  - TDD: test event parsing, test correct DB updates per event type

  **Must NOT do**:
  - Do not use RpcClient for log subscription — must use PubsubClient (WebSocket)

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 7 (with Tasks 38-40)
  - **Blocks**: Task 42 (main.rs spawns workers)
  - **Blocked By**: Tasks 5 (AppState), 29 (WS channels), 32 (Solana Pay for refund), 34 (refund service)

  **References**:
  - `pactum_backend_spec.md` lines 1958-1995 — §12.1 event_listener worker code
  - `pactum_backend_spec.md` lines 1492-1539 — §9.5 initiate_refund_if_eligible code

  **Acceptance Criteria**:
  - [ ] Subscribes to program logs via WebSocket
  - [ ] Correctly dispatches CreateAgreement, SignAgreement, Cancel, Expire, VoteRevoke events
  - [ ] Cancel/Expire triggers refund initiation
  - [ ] Auto-reconnects on disconnect
  - [ ] `cargo test -- workers::event_listener` passes

  **QA Scenarios:**
  ```
  Scenario: Event dispatch
    Tool: Bash
    Steps:
      1. Run `cargo test -- workers::event_listener::tests::test_handle_confirmed_tx`
      2. Verify CreateAgreement updates agreement_parties
      3. Verify CancelAgreement triggers refund_if_eligible
    Expected Result: Events correctly dispatched
    Evidence: .sisyphus/evidence/task-37-event-listener.txt
  ```

  **Commit**: YES (groups with Wave 7)
  - Message: `feat(workers): event listener with log subscription + refund trigger`
  - Files: `src/workers/event_listener.rs`

- [ ] 38. **Keeper Worker — 8 Scan Jobs (workers/keeper.rs)**

  **What to do**:
  - Create `src/workers/keeper.rs` per spec §12.2:
    - Runs every 60 seconds with `tokio::time::interval`
    - Scan 1: `expire_stale_agreements()` — atomic status transition to 'expiring', build + submit expire_agreement TX, revert on failure (M-3 idempotency)
    - Scan 2: `send_invitation_reminders()` — reminder_count=0 AND older than INVITE_REMINDER_AFTER_SECONDS
    - Scan 3: `expire_stale_invitations()` — status='pending' AND expires_at < now()
    - Scan 4: `check_hot_wallet_balances()` — vault SOL + treasury USDC/USDT/PYUSD alerts, circuit breaker
    - Scan 5: `sweep_treasury_excess()` — daily sweep above float threshold to cold wallet
    - Scan 6: `expire_timed_out_payments()` — pending payments older than 15 min
    - Scan 7: `reconcile_late_payments()` — check chain for late arrivals on expired payments (M-4)
    - Scan 8: `cleanup_expired_auth_records()` — delete expired siws_nonces (>5 min) + refresh_tokens
  - TDD: test each scan independently with mocked state

  **Must NOT do**:
  - Do not skip circuit breaker check — process.exit(1) if vault below threshold

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 7
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 5 (AppState), 12 (Solana service), 32 (Solana Pay), 34 (refund)

  **References**:
  - `pactum_backend_spec.md` lines 1997-2234 — §12.2 all 8 keeper scan functions with code
  - `pactum_backend_spec.md` lines 2033-2061 — expire_stale_agreements with M-3 idempotency
  - `pactum_backend_spec.md` lines 2063-2095 — check_hot_wallet_balances with circuit breaker

  **Acceptance Criteria**:
  - [ ] All 8 scan functions compile and run
  - [ ] Expire uses atomic 'expiring' status transition (M-3)
  - [ ] Circuit breaker triggers process.exit(1) below threshold
  - [ ] `cargo test -- workers::keeper` passes

  **QA Scenarios:**
  ```
  Scenario: Keeper scans execute
    Tool: Bash
    Steps:
      1. Run `cargo test -- workers::keeper::tests`
      2. Verify expire_stale_agreements uses atomic status transition
      3. Verify cleanup removes expired nonces and refresh tokens
    Expected Result: All 8 scans work correctly
    Evidence: .sisyphus/evidence/task-38-keeper.txt
  ```

  **Commit**: YES (groups with Wave 7)
  - Message: `feat(workers): keeper with 8 scan jobs`
  - Files: `src/workers/keeper.rs`

- [ ] 39. **Refund Worker (workers/refund_worker.rs)**

  **What to do**:
  - Create `src/workers/refund_worker.rs` per spec §9.5:
    - Polls every 30 seconds with `tokio::time::interval`
    - Fetch all payments WHERE status='refund_pending'
    - For each: call `execute_refund()` from refund service (Task 34)
    - On success: update status='refunded', store refund_tx_signature, set refund_completed_at, enqueue refund notification
    - On failure: log error, retry next cycle (creator ATA may be temporarily unavailable)
  - TDD: test refund execution + DB update, test failure retry

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 7
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 34 (refund service), 5 (AppState)

  **References**:
  - `pactum_backend_spec.md` lines 1541-1582 — §9.5 refund_worker exact code

  **Acceptance Criteria**:
  - [ ] Polls refund_pending payments every 30s
  - [ ] Successful refund updates status='refunded' with tx signature
  - [ ] Failed refund logged, retried next cycle
  - [ ] `cargo test -- workers::refund_worker` passes

  **QA Scenarios:**
  ```
  Scenario: Refund execution
    Tool: Bash
    Steps:
      1. Run `cargo test -- workers::refund_worker::tests`
      2. Verify refund_pending payment transitions to refunded
      3. Verify refund_tx_signature is stored
    Expected Result: Refund completes and records signature
    Evidence: .sisyphus/evidence/task-39-refund-worker.txt
  ```

  **Commit**: YES (groups with Wave 7)
  - Message: `feat(workers): refund worker with SPL transfer execution`
  - Files: `src/workers/refund_worker.rs`

- [ ] 40. **Notification Worker (workers/notification_worker.rs)**

  **What to do**:
  - Create `src/workers/notification_worker.rs` per spec §12.3:
    - Polls every 5 seconds with `tokio::time::interval`
    - Fetch 10 pending notification jobs
    - For each: attempt WS delivery first (instant, zero cost), then email if contact exists
    - WS delivery: `send_to_user()` from WS handler
    - Email delivery: use resend-rs to send via Resend API with appropriate template/subject
    - SIWS users without email: WS-only notification (log debug message)
    - On success: mark_sent(), On failure: increment_attempts()
  - TDD: test dispatch routes to WS + email, test email-less users get WS only

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 7
  - **Blocks**: Task 42
  - **Blocked By**: Tasks 13 (notification service), 5 (AppState)

  **References**:
  - `pactum_backend_spec.md` lines 2236-2277 — §12.3 notification_worker dispatch code
  - `pactum_backend_spec.md` lines 2279-2295 — §12.4 event types + email subjects table

  **Acceptance Criteria**:
  - [ ] Dispatches via WS + email (if available)
  - [ ] SIWS users without email receive WS-only
  - [ ] mark_sent/increment_attempts work correctly
  - [ ] `cargo test -- workers::notification_worker` passes

  **QA Scenarios:**
  ```
  Scenario: Notification dispatch
    Tool: Bash
    Steps:
      1. Run `cargo test -- workers::notification_worker::tests`
      2. Verify user with email gets WS + email dispatch
      3. Verify user without email gets WS-only dispatch
    Expected Result: Correct dispatch per contact availability
    Evidence: .sisyphus/evidence/task-40-notification-worker.txt
  ```

  **Commit**: YES (groups with Wave 7)
  - Message: `feat(workers): notification worker with WS + email dispatch`
  - Files: `src/workers/notification_worker.rs`

- [ ] 41. **docker-compose.yml + Dockerfile**

  **What to do**:
  - Create `docker-compose.yml` per spec §13:
    - `api` service: build from `api/Dockerfile`, port 8080, env vars from shell, secrets mount (db_password, vault_keypair, treasury_keypair), depends_on postgres with healthcheck
    - `postgres` service: postgres:16-alpine, POSTGRES_PASSWORD_FILE from secret, volumes for data + migrations, healthcheck with pg_isready
    - Secrets section: external secrets for db_password, vault_keypair, treasury_keypair
    - Volumes: pg_data
  - Create `api/Dockerfile` per spec §13:
    - Multi-stage build: rust:1.82-slim builder → debian:bookworm-slim runtime
    - Cache dependencies layer (empty main.rs trick)
    - Install ca-certificates in runtime
    - EXPOSE 8080, CMD ["pactum-backend"]
  - TDD: test Docker build succeeds

  **Must NOT do**:
  - Do not hardcode secrets in docker-compose.yml
  - Do not expose PostgreSQL port externally

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 8 (with Tasks 42-44)
  - **Blocks**: Task F3 (QA uses Docker)
  - **Blocked By**: ALL previous tasks (needs compilable binary)

  **References**:
  - `pactum_backend_spec.md` lines 2299-2370 — §13 docker-compose.yml exact content
  - `pactum_backend_spec.md` lines 2374-2398 — api/Dockerfile exact content

  **Acceptance Criteria**:
  - [ ] `docker-compose build` succeeds
  - [ ] `docker-compose up -d` starts api + postgres
  - [ ] PostgreSQL healthcheck passes
  - [ ] API container responds on port 8080

  **QA Scenarios:**
  ```
  Scenario: Docker compose starts
    Tool: Bash
    Steps:
      1. Run `docker-compose build`
      2. Run `docker-compose up -d`
      3. Wait 30s for startup
      4. Run `docker-compose ps` and verify both services are 'running'
      5. Run `curl -s http://localhost:8080/auth/challenge`
      6. Verify response contains nonce
    Expected Result: Both services running, API responds
    Failure Indicators: Build failure, health check failing, connection refused
    Evidence: .sisyphus/evidence/task-41-docker.txt
  ```

  **Commit**: YES (groups with Wave 8)
  - Message: `feat(docker): docker-compose + multi-stage Dockerfile`
  - Files: `docker-compose.yml`, `api/Dockerfile`

- [ ] 42. **main.rs Final: Wire ALL Routes, Spawn Workers, Startup Validation**

  **What to do**:
  - Expand `src/main.rs` (from Task 18 MVP) to wire ALL routes:
    - auth_routes (challenge, verify, refresh, logout, OAuth, link_wallet)
    - upload_routes
    - agreement_routes (create, sign, cancel, expire, revoke, retract, close, read, list)
    - draft_routes (get, delete, reinvite, submit)
    - invite_routes (get, accept)
    - payment_routes (initiate, status)
    - user_routes (me, contacts)
    - ws_routes
  - Spawn all 4 background workers as tokio tasks:
    - `tokio::spawn(event_listener::run(state.clone()))`
    - `tokio::spawn(keeper::run(state.clone()))`
    - `tokio::spawn(refund_worker::run(state.clone()))`
    - `tokio::spawn(notification_worker::run(state.clone()))`
  - Add startup validation sequence: keypair validation → DB migration → worker spawn → server bind
  - Build StablecoinRegistry from config and add to AppState
  - TDD: test full server starts with all routes

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 8
  - **Blocks**: Task 43 (integration tests)
  - **Blocked By**: ALL handler + worker tasks (18-40)

  **References**:
  - `pactum_backend_spec.md` lines 557-588 — §6 router.rs with all route groups
  - `pactum_backend_spec.md` lines 228-262 — §4 AppState full definition

  **Acceptance Criteria**:
  - [ ] All route groups wired
  - [ ] All 4 workers spawned
  - [ ] `cargo build --release` succeeds
  - [ ] `cargo clippy -- -D warnings` passes

  **QA Scenarios:**
  ```
  Scenario: Full server with all routes
    Tool: Bash
    Steps:
      1. Run `cargo build --release`
      2. Start server in background
      3. Verify `curl /auth/challenge` works
      4. Verify `curl /agreement/test-pda` works (returns 404 or chain error)
      5. Verify `curl /payment/status/test-draft` works (returns 401)
    Expected Result: All route groups respond
    Evidence: .sisyphus/evidence/task-42-full-server.txt
  ```

  **Commit**: YES (groups with Wave 8)
  - Message: `feat: main.rs final — all routes, workers, startup validation`
  - Files: `src/main.rs`

- [ ] 43. **Integration Test Suite: MVP End-to-End Flow**

  **What to do**:
  - Create `tests/integration_test.rs` with MVP flow:
    - Start server (or use axum::TestServer)
    - Test 1: SIWS flow — challenge → verify → receive tokens
    - Test 2: Create agreement — POST /agreement with all-resolved parties → receive TX
    - Test 3: Sign agreement — POST /agreement/{pda}/sign → receive unsigned TX
    - Test 4: Refresh token — POST /auth/refresh → receive new tokens
    - Test 5: Logout — POST /auth/logout → 204
  - Use real PostgreSQL instance (via docker-compose.test.yml or test container)
  - Mock Solana RPC for TX submission (or use devnet)
  - TDD: all integration tests must pass end-to-end

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`coding-guidelines`, `test-driven-development`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 8
  - **Blocks**: Task F1-F4
  - **Blocked By**: Task 42 (full server)

  **References**:
  - All spec sections — this is the integration point

  **Acceptance Criteria**:
  - [ ] MVP flow: auth → create → sign → complete
  - [ ] All 5 integration tests pass
  - [ ] `cargo test --test integration_test` passes

  **QA Scenarios:**
  ```
  Scenario: MVP end-to-end flow
    Tool: Bash
    Preconditions: PostgreSQL running, migrations applied
    Steps:
      1. Run `cargo test --test integration_test`
      2. Verify SIWS challenge → verify → tokens
      3. Verify POST /agreement returns TX
      4. Verify POST /agreement/{pda}/sign returns unsigned TX
    Expected Result: Full MVP flow passes
    Evidence: .sisyphus/evidence/task-43-integration.txt
  ```

  **Commit**: YES (groups with Wave 8)
  - Message: `test: MVP integration test suite — auth → agreement → sign`
  - Files: `tests/integration_test.rs`

- [ ] 44. **.gitignore + .env.example Finalization + sqlx prepare**

  **What to do**:
  - Create `.gitignore` with:
    - `/target`, `*.json` (in keys directory), `.env`, `docker-compose.override.yml`, `*keypair*`, `arweave-wallet.json`, `.sqlx/` (if not checking in prepared queries)
  - Verify `.env.example` is complete (all ~60 vars from spec §4)
  - Run `sqlx prepare` to generate offline query metadata in `.sqlx/` directory
  - Verify all sqlx compile-time checked queries work offline
  - Final `cargo clippy -- -D warnings` + `cargo test` pass

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`coding-guidelines`]

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 8
  - **Blocks**: Task F1-F4
  - **Blocked By**: Task 42 (full server needed for sqlx prepare)

  **References**:
  - `pactum_backend_spec.md` lines 142-226 — §4 complete .env.example
  - `pactum_backend_spec.md` lines 2372 — .gitignore requirement

  **Acceptance Criteria**:
  - [ ] `.gitignore` prevents accidental secret commits
  - [ ] `.env.example` has all ~60 variables
  - [ ] `sqlx prepare` succeeds
  - [ ] `cargo clippy -- -D warnings` passes
  - [ ] `cargo test` passes all tests

  **QA Scenarios:**
  ```
  Scenario: Final build verification
    Tool: Bash
    Steps:
      1. Run `cargo build --release`
      2. Run `cargo clippy -- -D warnings`
      3. Run `cargo test`
      4. Verify .gitignore covers keypair files
    Expected Result: Clean build, zero warnings, all tests pass
    Evidence: .sisyphus/evidence/task-44-final-verify.txt
  ```

  **Commit**: YES (groups with Wave 8)
  - Message: `chore: .gitignore, .env.example finalization, sqlx prepare`
  - Files: `.gitignore`, `.env.example`, `.sqlx/`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Rejection → fix → re-run.

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, curl endpoint, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files exist in .sisyphus/evidence/. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Skills: [`coding-guidelines`]
  Run `cargo build --release && cargo clippy -- -D warnings && cargo test`. Review all source files for: `as any`/`unwrap()` in prod, empty catches, `println!` in prod, commented-out code, unused imports. Check Rust coding guidelines compliance: newtypes, `thiserror`, no `lazy_static!`, proper naming. Check AI slop: excessive comments, over-abstraction, generic names.
  Output: `Build [PASS/FAIL] | Clippy [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [ ] F3. **Real Manual QA** — `unspecified-high`
  Start from clean state (`docker-compose down -v && docker-compose up -d`). Execute EVERY QA scenario from EVERY task — follow exact steps, capture evidence. Test cross-task integration (auth → agreement → sign flow). Test edge cases: expired nonce, invalid signature, duplicate party. Save to `.sisyphus/evidence/final-qa/`.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual diff (git log/diff). Verify 1:1 — everything in spec was built (no missing), nothing beyond spec was built (no creep). Check "Must NOT do" compliance. Detect cross-task contamination. Flag unaccounted changes. Verify all spec sections §1-§14 have corresponding implementation.
  Output: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Unaccounted [CLEAN/N files] | VERDICT`

---

## Commit Strategy

| Wave | Commit | Message | Pre-commit |
|------|--------|---------|-----------|
| 1 | After all 7 tasks | `feat: project foundation — scaffolding, config, migrations, state, router` | `cargo check` |
| 2 | After all 6 tasks | `feat: core services — hash, crypto, keypair, jwt, solana, notification` | `cargo check` |
| 3 | After all 5 tasks | `feat: auth MVP — SIWS, JWT middleware, refresh, user handlers` | `cargo test` |
| 4 | After all 5 tasks | `feat: agreement MVP — create, sign, read, Solana TX construction` | `cargo test` |
| 5 | After all 6 tasks | `feat: expansion — OAuth2, upload, storage, metadata, WebSocket` | `cargo test` |
| 6 | After all 7 tasks | `feat: drafts, invitations, payment, refund, cancel/expire/revoke` | `cargo test` |
| 7 | After all 4 tasks | `feat: background workers — event listener, keeper, refund, notification` | `cargo test` |
| 8 | After all 4 tasks | `feat: Docker, integration tests, final wiring` | `cargo test && docker-compose build` |

---

## Success Criteria

### Verification Commands
```bash
cargo build --release          # Expected: Compiling pactum-backend ... Finished
cargo clippy -- -D warnings    # Expected: no warnings
cargo test                     # Expected: test result: ok. N passed; 0 failed
docker-compose up -d           # Expected: api and postgres containers running
curl http://localhost:8080/auth/challenge  # Expected: {"nonce":"<uuid>"}
```

### Final Checklist
- [ ] All "Must Have" present (per §8 API routes, §5 migrations, §11 services, §12 workers)
- [ ] All "Must NOT Have" absent (no Stripe, no Apple OAuth, no lazy_static, no unwrap)
- [ ] All tests pass (`cargo test`)
- [ ] Clippy clean (`cargo clippy -- -D warnings`)
- [ ] Docker build succeeds
- [ ] MVP flow verified end-to-end
