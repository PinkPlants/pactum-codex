# Docker Setup for Pactum Backend

This directory contains the API Dockerfile and development override compose file. The primary orchestration file is at the repository root (`../docker-compose.yml`).

## Quick Start

### 1. Prepare Secret Files (Compose Secrets)

Run from repository root:

```bash
mkdir -p api/secrets

# Database password
openssl rand -base64 32 > api/secrets/db_password.txt

# Platform keypairs
cp /path/to/vault_keypair.json api/secrets/vault_keypair.json
cp /path/to/treasury_keypair.json api/secrets/treasury_keypair.json

# Arweave wallet
cp /path/to/arweave-wallet.json api/secrets/arweave_wallet.json
```

### 2. Configure Environment

```bash
cp .env.example .env
# Edit .env with your actual values
```

### 3. Local Build + Host Compose

```bash
# Build api image from source
docker compose build api

# Start full stack
docker compose up -d
```

### 4. Development Mode (Hot Reload)

```bash
docker compose -f docker-compose.yml -f api/docker-compose.dev.yml up api
```

### 5. Remote DockerHub Images (No Build)

```bash
export PACTUM_API_IMAGE=univer5al/pactum-codex:latest
export PACTUM_API_PULL_POLICY=always

docker compose pull --include-deps
docker compose up -d --no-build
```

## Services

| Service | Description | Port |
|---------|-------------|------|
| `api` | Pactum Backend API | 8080 |
| `postgres` | PostgreSQL 16 database | 5432 (local only) |

## Architecture

### Multi-Stage Dockerfile

The `Dockerfile` uses three stages:

1. **deps**: Caches dependencies for faster rebuilds
2. **builder**: Compiles the Rust application
3. **runtime**: Minimal production image with security hardening

### Security Features

- **Non-root user**: Runs as UID 10001 (`pactum` user)
- **File-backed Compose secrets**: Sensitive data mounted to `/run/secrets/*`
- **Minimal base image**: `debian:bookworm-slim` with runtime dependencies only
- **Health checks**: Built-in container health monitoring
- **Resource limits**: CPU and memory constraints in compose config

## Development Workflow

### Hot Reload

The development override mounts source code and runs `cargo watch`:

```bash
docker compose -f docker-compose.yml -f api/docker-compose.dev.yml up api
```

Changes to `src/` are automatically recompiled.

### Database Access

```bash
# Connect via psql
docker exec -it pactum-postgres psql -U pactum -d pactum
```

### Viewing Logs

```bash
# All services
docker compose logs -f

# Specific service
docker compose logs -f api
```

## DockerHub Publishing

```bash
# Authenticate
docker login

# Build local image
docker compose build api

# Tag version
export VERSION=v0.1.0
docker tag univer5al/pactum-codex:latest univer5al/pactum-codex:${VERSION}

# Push latest + version
docker push univer5al/pactum-codex:latest
docker push univer5al/pactum-codex:${VERSION}
```

Optional multi-arch publish:

```bash
docker buildx build --platform linux/amd64,linux/arm64 \
  -f api/Dockerfile --target runtime \
  -t univer5al/pactum-codex:latest \
  --push .
```

## Production Deployment

### Host Deployment

```bash
docker compose -f docker-compose.yml up -d
```

### Docker Swarm Deployment

```bash
docker stack deploy -c docker-compose.yml pactum
```

### Health Check

```bash
curl http://localhost:8080/health
```

## Troubleshooting

### Secret Files Not Found

```bash
ls -la api/secrets
```

Required files:

- `api/secrets/db_password.txt`
- `api/secrets/vault_keypair.json`
- `api/secrets/treasury_keypair.json`
- `api/secrets/arweave_wallet.json`

### Database Connection Failed

```bash
docker compose ps
docker compose logs postgres
docker compose down && docker compose up -d
```

### Build Cache Issues

```bash
docker compose build --no-cache
docker compose down -v
docker system prune -a
```

## File Structure

```text
.
├── Dockerfile                  # Multi-stage production Dockerfile
├── docker-compose.dev.yml      # Development overrides
├── secrets/                    # Local secret files (not committed)
└── README.md                   # This file

../docker-compose.yml           # Root production orchestration
```
