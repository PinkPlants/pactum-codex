# Pactum Backend

A production-ready Rust backend for Pactum — a Solana-based multi-party agreement platform with NFT credentials and stablecoin payments.

## Overview

Pactum enables parties to create, negotiate, and execute legally-binding agreements on Solana with:

- **Multi-party digital agreements** with on-chain signatures and NFT credentials
- **Stablecoin payments** (USDC, USDT, PYUSD) — no SOL volatility exposure
- **Wallet-based authentication** with OAuth fallbacks (Google, Microsoft)
- **Permanent document storage** via IPFS/Arweave
- **Real-time notifications** via WebSocket
- **Automated expiry handling** and refund mechanisms

## Program IDL Compliance

This backend is fully compliant with the Pactum Solana program IDL (v0.1.0):

- **Program ID**: `DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P`
- **MPL Core**: `CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d`

### Supported Instructions

| Instruction | Status | Accounts | Description |
|-------------|--------|----------|-------------|
| `create_agreement` | Implemented | 5 | Initialize agreement PDA with parties |
| `sign_agreement` | Implemented | 9 | Party signs, mints NFT on final signature |
| `cancel_agreement` | Implemented | 4 | Creator cancels pending agreement |
| `expire_agreement` | Implemented | 4 | Vault expires agreement past deadline |
| `vote_revoke` | Implemented | 8 | Party votes to revoke completed agreement |
| `retract_revoke_vote` | Implemented | 3 | Party retracts their revoke vote |
| `initialize_collection` | Implemented | 7 | Create MPL Core collection for creator |

### PDA Seeds

- **Agreement**: `["agreement", creator, agreement_id]`
- **Collection State**: `["collection", creator]`
- **PDA Authority**: `["mint_authority", "v1", vault_funder]`

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Pactum Backend                                │
├─────────────────────────────────────────────────────────────────────────┤
│  Axum Web Server │ Rate Limiting │ CORS │ JWT Auth │ Wallet Guard      │
├─────────────────────────────────────────────────────────────────────────┤
│  Handlers:                                                              │
│  • auth          - SIWS + OAuth (Google, Microsoft)                     │
│  • agreement     - Create, sign, cancel, revoke                         │
│  • draft         - Pre-chain draft lifecycle                            │
│  • invite        - Party invitation by email                            │
│  • payment       - Stablecoin payment processing                        │
│  • upload        - Document upload + hash verification                  │
│  • user          - Profile, contacts, preferences                       │
│  • ws            - Real-time WebSocket events                           │
├─────────────────────────────────────────────────────────────────────────┤
│  Services:                                                              │
│  • solana        - Program IDL-compliant transaction builders           │
│  • solana_pay    - USDC/USDT/PYUSD payment processing                   │
│  • storage       - IPFS/Arweave document storage                        │
│  • crypto        - AES-256-GCM PII encryption                           │
│  • jwt           - Token generation/validation                          │
│  • metadata      - NFT metadata JSON generation                         │
├─────────────────────────────────────────────────────────────────────────┤
│  Workers:                                                               │
│  • event_listener    - Solana logsSubscribe for on-chain events         │
│  • expiry_worker     - Submits expire_agreement for past-due agreements │
│  • keeper            - Treasury sweeps, invitation cleanup              │
│  • refund_worker     - Automated stablecoin refunds                     │
│  • notification_worker - Email/push notification dispatch               │
├─────────────────────────────────────────────────────────────────────────┤
│  PostgreSQL  │  Solana RPC  │  IPFS/Arweave  │  Resend (Email)        │
└─────────────────────────────────────────────────────────────────────────┘
```

## Tech Stack

| Category | Technology |
|----------|-----------|
| Web Framework | [axum](https://github.com/tokio-rs/axum) 0.8 |
| Async Runtime | [tokio](https://tokio.rs) |
| Database | PostgreSQL 16 + [sqlx](https://github.com/launchbadge/sqlx) |
| Auth | JWT + OAuth2 (Google, Microsoft) + SIWS |
| Blockchain | Solana Mainnet + MPL Core |
| Storage | IPFS / Arweave |
| Email | Resend |

## Project Structure

```
├── src/
│   ├── handlers/          # HTTP route handlers
│   │   ├── auth.rs        # SIWS wallet auth, OAuth, JWT
│   │   ├── agreement.rs   # Create, sign, cancel, revoke agreements
│   │   ├── draft.rs       # Agreement draft lifecycle (pre-chain)
│   │   ├── invite.rs      # Party invitations by email
│   │   ├── payment.rs     # Payment initiation and webhooks
│   │   ├── upload.rs      # Document upload + hash verification
│   │   ├── user.rs        # Profile, contacts, preferences
│   │   └── ws.rs          # WebSocket real-time events
│   ├── services/          # Business logic
│   │   ├── solana.rs      # Program interaction (IDL-compliant)
│   │   ├── solana_pay.rs  # Stablecoin payment processing
│   │   ├── storage.rs     # IPFS/Arweave uploads
│   │   ├── crypto.rs      # PII encryption
│   │   ├── jwt.rs         # Token generation/validation
│   │   ├── metadata.rs    # NFT metadata generation
│   │   └── ...
│   ├── middleware/        # auth.rs, wallet_guard.rs
│   ├── workers/           # Background tasks
│   │   ├── event_listener.rs
│   │   ├── expiry_worker.rs
│   │   ├── keeper.rs
│   │   ├── refund_worker.rs
│   │   └── notification_worker.rs
│   ├── solana_types.rs    # IDL-matching on-chain types
│   ├── router.rs          # Route definitions
│   ├── state.rs           # AppState with ProtectedKeypair
│   ├── config.rs          # Environment configuration
│   └── main.rs
├── migrations/            # sqlx database migrations
├── Cargo.toml
└── .env.example          # Configuration template
```

## Quick Start

### Prerequisites

- Rust 1.75+
- PostgreSQL 16+
- Solana CLI (optional, for testing)

### 1. Clone and Setup

```bash
git clone <repo>
cd pactum-backend
cp .env.example .env
# Edit .env with your values
```

### 2. Database Setup

```bash
# Create database
createdb pactum

# Run migrations (automatic on startup)
cargo sqlx migrate run
```

### 3. Keypair Setup

```bash
# Generate platform keypairs (keep these secure!)
solana-keygen new -o /run/secrets/vault_keypair.json
solana-keygen new -o /run/secrets/treasury_keypair.json

# Update .env with pubkey values
export PLATFORM_VAULT_PUBKEY=$(solana-keygen pubkey /run/secrets/vault_keypair.json)
export PLATFORM_TREASURY_PUBKEY=$(solana-keygen pubkey /run/secrets/treasury_keypair.json)
```

### 4. Run

```bash
# Development
cargo run

# With hot reload
cargo watch -x run

# Production build
cargo build --release
```

## API Overview

### Authentication

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `GET` | `/auth/challenge` | — | Get SIWS nonce |
| `POST` | `/auth/verify` | — | Verify wallet signature → JWT |
| `GET` | `/auth/oauth/google` | — | Redirect to Google OAuth |
| `GET` | `/auth/oauth/google/callback` | — | OAuth callback → JWT |
| `GET` | `/auth/oauth/microsoft` | — | Redirect to Microsoft OAuth |
| `GET` | `/auth/oauth/microsoft/callback` | — | OAuth callback → JWT |
| `POST` | `/auth/link/wallet` | JWT (OAuth) | Link wallet to OAuth account |
| `POST` | `/auth/refresh` | — | Rotate refresh token |
| `POST` | `/auth/logout` | — | Revoke refresh token |

### Agreements

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `POST` | `/agreement` | JWT + wallet | Create agreement (builds tx) |
| `GET` | `/agreement/:pda` | — | Get agreement state from chain |
| `GET` | `/agreements` | JWT | List user's agreements |
| `POST` | `/agreement/:pda/sign` | JWT + wallet | Build sign_agreement tx |
| `POST` | `/agreement/:pda/cancel` | JWT + wallet | Build cancel_agreement tx |
| `POST` | `/agreement/:pda/revoke` | JWT + wallet | Build vote_revoke tx |
| `POST` | `/agreement/:pda/retract` | JWT + wallet | Build retract_revoke_vote tx |

### Drafts (Pre-Chain)

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `GET` | `/draft/:id` | JWT | Get draft status |
| `DELETE` | `/draft/:id` | JWT (creator) | Discard draft |
| `PUT` | `/draft/:id/reinvite` | JWT (creator) | Resend invitation |
| `POST` | `/draft/:id/submit` | JWT + wallet | Upload doc + build create_agreement tx |

### Invitations

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `GET` | `/invite/:token` | — | Validate invitation |
| `POST` | `/invite/:token/accept` | JWT + wallet | Accept invitation, link wallet |

### Upload

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `POST` | `/upload` | JWT + wallet | Upload document, verify hash, return URI |

### Payments

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `POST` | `/payment/initiate/:draft_id` | JWT | Start stablecoin payment |
| `GET` | `/payment/status/:draft_id` | JWT | Check payment status |

### User

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| `GET` | `/user/me` | JWT | Get profile |
| `PUT` | `/user/me` | JWT | Update display_name |
| `PUT` | `/user/contacts` | JWT | Update encrypted contacts |
| `DELETE` | `/user/contacts` | JWT | Remove contacts |

### WebSocket

Connect to `/ws` for real-time events:

```javascript
const ws = new WebSocket('wss://api.pactum.app/ws', ['jwt', access_token]);

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  // Events: AgreementCreated, AgreementSigned, AgreementCompleted,
  //         AgreementCancelled, AgreementExpired, DraftReady, etc.
};
```

## Transaction Building

The backend builds partially-signed transactions that clients complete:

1. **Backend** builds tx with vault signature (fee payer)
2. **Client** receives base64-encoded tx
3. **Client** deserializes, signs with user wallet
4. **Client** submits to Solana RPC

Example:
```rust
// Backend
let tx = build_create_agreement_tx(
    rpc, &args, &creator, &vault_keypair, config
).await?;
// Returns base64-encoded partially-signed transaction

// Client
const tx = Transaction.from(Buffer.from(base64Tx, 'base64'));
await wallet.signTransaction(tx);
await connection.sendRawTransaction(tx.serialize());
```

## Configuration

See `.env.example` for all options:

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string |
| `SOLANA_RPC_URL` | Solana JSON-RPC endpoint |
| `SOLANA_WS_URL` | Solana WebSocket endpoint (logsSubscribe) |
| `PROGRAM_ID` | Pactum program public key |
| `JWT_SECRET` | JWT signing key (256-bit hex) |
| `PLATFORM_VAULT_KEYPAIR_PATH` | Path to vault keypair |
| `PLATFORM_TREASURY_KEYPAIR_PATH` | Path to treasury keypair |
| `STABLECOIN_USDC_MINT` | USDC mint address |
| `STABLECOIN_USDT_MINT` | USDT mint address |
| `STABLECOIN_PYUSD_MINT` | PYUSD mint address |
| `IPFS_API_URL` / `IPFS_JWT` | IPFS credentials |
| `RESEND_API_KEY` | Email service API key |

## Security Model

- **Dual hot wallet architecture**:
  - **Vault**: Holds SOL, pays gas, low float (1-2 SOL), blast radius limited
  - **Treasury**: Holds stablecoins, signs refunds, swept daily
- **Encrypted PII**: Email/phone encrypted with AES-256-GCM
- **Blind indexing**: HMAC-based email lookup (no plaintext)
- **Short-lived JWTs**: 15-minute access tokens, 7-day refresh rotation
- **Rate limiting**: Per-IP limits via tower-governor
- **ProtectedKeypair**: Newtype wrapper prevents keypair exposure in logs

## Development

```bash
# Format
cargo fmt

# Lint
cargo clippy

# Test
cargo test

# Database migration
cargo sqlx migrate add <name>

# Check IDL compliance
cargo test services::solana::tests
```

## License

[LICENSE](./LICENSE)

## Support

For issues and feature requests, please open a GitHub issue.
