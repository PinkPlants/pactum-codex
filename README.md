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
├── api/
│   ├── Dockerfile              # Production multi-stage Dockerfile
│   ├── docker-compose.dev.yml  # Development overrides
│   └── README.md               # Docker setup guide
├── migrations/            # sqlx database migrations
├── docker-compose.yml     # Production orchestration
├── Cargo.toml
└── .env.example          # Configuration template
```

## Quick Start

Choose your preferred deployment method:

### Option A: Docker (Recommended)

The fastest way to run Pactum with PostgreSQL and migrations pre-wired.

#### Prerequisites

- Docker 20.10+
- Docker Compose plugin (`docker compose`)

Default images are hosted on DockerHub:
```bash
docker pull univer5al/pactum-codex:latest
```

#### 1. Clone and Configure

```bash
git clone <repo>
cd pactum-codex
cp .env.example .env
# Edit .env with your values (RPC URLs, OAuth credentials, etc.)
```

#### 2. Create Secret Files for Compose

```bash
mkdir -p api/secrets

# Database password file
openssl rand -base64 32 > api/secrets/db_password.txt

# Platform keypairs (generate if needed)
solana-keygen new -o vault_keypair.json
solana-keygen new -o treasury_keypair.json
cp ./vault_keypair.json api/secrets/vault_keypair.json
cp ./treasury_keypair.json api/secrets/treasury_keypair.json

# Arweave wallet file (required by compose defaults)
cp ./arweave-wallet.json api/secrets/arweave_wallet.json
```

#### 3. Build and Run with Host Compose

```bash
# Build local images from source
docker compose build api

# Run in detached mode on host
docker compose up -d
```

The API will be available at `http://localhost:8080`.

#### 4. Development Override (Hot Reload)

```bash
docker compose -f docker-compose.yml -f api/docker-compose.dev.yml up api
```

#### 5. Run from Remote DockerHub Images

```bash
# Optional: pin versions
export PACTUM_API_IMAGE=univer5al/pactum-codex:latest

# Force pulls, then start without building
export PACTUM_API_PULL_POLICY=always
docker compose pull --include-deps
docker compose up -d --no-build
```

To switch back to local builds, unset pull policy and rebuild:

```bash
unset PACTUM_API_PULL_POLICY
docker compose build api
```

See [`api/README.md`](api/README.md) for detailed Docker instructions.

---

### Option B: Native Development

For local development without Docker.

#### Prerequisites

- Rust 1.88+
- PostgreSQL 16+
- Solana CLI (optional, for testing)

#### 1. Clone and Setup

```bash
git clone <repo>
cd pactum-codex
cp .env.example .env
# Edit .env with your values
```

#### 2. Database Setup

```bash
# Create database
createdb pactum

# Run migrations (automatic on startup)
cargo sqlx migrate run
```

#### 3. Keypair Setup

```bash
# Generate platform keypairs (keep these secure!)
solana-keygen new -o /run/secrets/vault_keypair.json
solana-keygen new -o /run/secrets/treasury_keypair.json

# Update .env with pubkey values
export PLATFORM_VAULT_PUBKEY=$(solana-keygen pubkey /run/secrets/vault_keypair.json)
export PLATFORM_TREASURY_PUBKEY=$(solana-keygen pubkey /run/secrets/treasury_keypair.json)
```

#### 4. Run

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
| `STABLECOIN_USDC_ATA` | Platform USDC associated token account |
| `STABLECOIN_USDT_MINT` | USDT mint address |
| `STABLECOIN_USDT_ATA` | Platform USDT associated token account |
| `STABLECOIN_PYUSD_MINT` | PYUSD mint address |
| `STABLECOIN_PYUSD_ATA` | Platform PYUSD associated token account |
| `PINATA_JWT` | Pinata IPFS JWT for uploads |
| `PINATA_GATEWAY_DOMAIN` | Pinata gateway domain for file access |
| `ARWEAVE_WALLET_PATH` | Path to Arweave wallet keypair |
| `RESEND_API_KEY` | Email service API key |
| `EMAIL_FROM` | Sender email address for notifications |
| `INVITE_BASE_URL` | Base URL for invitation links |
| `PLATFORM_FEE_USD_CENTS` | Per-agreement fee in cents |
| `PLATFORM_FEE_FREE_TIER` | Lifetime free agreements per user |
| `VAULT_MIN_SOL_ALERT` | Alert threshold for vault SOL balance |
| `TREASURY_SWEEP_DEST` | Cold wallet address for treasury sweeps |

## Security Model

- **Dual hot wallet architecture**:
  - **Vault**: Holds SOL, pays gas, low float (1-2 SOL), blast radius limited
  - **Treasury**: Holds stablecoins, signs refunds, swept daily
- **Encrypted PII**: Email/phone encrypted with AES-256-GCM
- **Blind indexing**: HMAC-based email lookup (no plaintext)
- **Short-lived JWTs**: 15-minute access tokens, 7-day refresh rotation
- **Rate limiting**: Per-IP limits via tower-governor
- **ProtectedKeypair**: Newtype wrapper prevents keypair exposure in logs

### Docker Build Notes

The Dockerfile has been optimized for reliable builds:

- **Rust 1.88+**: Required for compatibility with newer dependencies
- **Multi-stage caching**: Dependencies cached in separate stage for faster rebuilds
- **LTO disabled**: Link Time Optimization disabled to prevent symbol conflicts with Solana crates
- **Artifact cleanup**: Dummy build artifacts removed to avoid entrypoint symbol conflicts
- **Migrations included**: SQLx migration files copied into build context

### Docker Security

- **Non-root containers**: Runs as UID 10001, not root
- **Docker secrets**: Keypairs never in environment variables or image layers
- **Minimal attack surface**: `debian:bookworm-slim` base with only runtime dependencies
- **No secrets in git**: `.gitignore` and `.dockerignore` exclude all keypair files
- **Resource limits**: CPU and memory constraints prevent resource exhaustion
- **Health checks**: Automatic container restart on failure

### Docker Image Usage and DockerHub Publishing

Use this sequence to publish the API runtime image:

```bash
# 1) Authenticate
docker login

# 2) Build local images with compose tags
docker compose build api

# 3) Tag release versions (example)
export VERSION=v0.1.0
docker tag univer5al/pactum-codex:latest univer5al/pactum-codex:${VERSION}

# 4) Push latest and version tags
docker push univer5al/pactum-codex:latest
docker push univer5al/pactum-codex:${VERSION}
```

Optional multi-arch publishing with Buildx:

```bash
docker buildx build --platform linux/amd64,linux/arm64 \
  -f api/Dockerfile --target runtime \
  -t univer5al/pactum-codex:latest \
  --push .
```

## Development

### Standard Development

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

### Docker Development

```bash
# Start with hot reload
docker compose -f docker-compose.yml -f api/docker-compose.dev.yml up

# View logs
docker compose logs -f api

# Rebuild after dependency changes
docker compose build --no-cache

# Reset everything (including database)
docker compose down -v
docker compose up -d

# Access PostgreSQL
docker exec -it pactum-postgres psql -U pactum -d pactum
```

### Production Deployment

```bash
# Deploy with Docker Swarm
docker stack deploy -c docker-compose.yml pactum

# Or run directly on a host
docker compose -f docker-compose.yml up -d

# Or force remote image mode on host
PACTUM_API_PULL_POLICY=always \
docker compose -f docker-compose.yml up -d --no-build
```

## License

[GPL-3.0](./LICENSE)

## Support

For issues and feature requests, please open a GitHub issue.
