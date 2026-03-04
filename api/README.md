# Docker Setup for Pactum Backend

This directory contains production-ready Docker configurations for the Pactum Backend.

## Quick Start

### 1. Create Docker Secrets

Before running the application, you must create the required Docker secrets:

```bash
# Database password
echo "$(openssl rand -base64 32)" | docker secret create db_password -

# Vault keypair (for Solana transactions)
docker secret create vault_keypair /path/to/vault_keypair.json

# Treasury keypair (for refunds)
docker secret create treasury_keypair /path/to/treasury_keypair.json

# Arweave wallet (optional, for document storage)
docker secret create arweave_wallet /path/to/arweave-wallet.json
```

### 2. Configure Environment

```bash
cp ../.env.example ../.env
# Edit .env with your actual values
```

### 3. Run Production Stack

```bash
docker-compose up -d
```

### 4. Run Development Stack (with hot reload)

```bash
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up
```

## Services

| Service | Description | Port |
|---------|-------------|------|
| `api` | Pactum Backend API | 8080 |
| `postgres` | PostgreSQL 16 database | 5432 (local only) |
| `migrations` | SQLx database migrations | - |

## Architecture

### Multi-Stage Dockerfile

The `Dockerfile` uses three stages:

1. **deps**: Caches dependencies for faster rebuilds
2. **builder**: Compiles the Rust application
3. **runtime**: Minimal production image with security hardening

### Security Features

- **Non-root user**: Runs as UID 10001 (`pactum` user)
- **Docker secrets**: Sensitive data never in environment variables
- **Minimal base image**: `debian:bookworm-slim` with only runtime dependencies
- **Health checks**: Built-in container health monitoring
- **Resource limits**: CPU and memory constraints defined

## Development Workflow

### Hot Reload

The development override mounts your source code as a volume:

```bash
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up api
```

Changes to `src/` are automatically recompiled.

### Database Access

Connect to the local PostgreSQL:

```bash
# Get the password
docker secret inspect --format='{{.Spec.Name}}' db_password

# Connect via psql
docker exec -it pactum-postgres psql -U pactum -d pactum
```

### Viewing Logs

```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f api
```

## Production Deployment

### Prerequisites

1. Docker Swarm or Kubernetes cluster
2. Docker secrets created on the cluster
3. Environment variables configured

### Deploy with Docker Swarm

```bash
# Initialize swarm (if not already)
docker swarm init

# Deploy stack
docker stack deploy -c docker-compose.yml pactum

# View services
docker stack services pactum
```

### Health Checks

The API includes a health check endpoint. Verify status:

```bash
curl http://localhost:8080/health
```

## Troubleshooting

### Secrets Not Found

If you see "secret not found" errors:

```bash
# List available secrets
docker secret ls

# Recreate missing secrets
echo "your-password" | docker secret create db_password -
```

### Database Connection Failed

```bash
# Check PostgreSQL is healthy
docker-compose ps

# View PostgreSQL logs
docker-compose logs postgres

# Restart stack
docker-compose down && docker-compose up -d
```

### Build Cache Issues

```bash
# Force rebuild without cache
docker-compose build --no-cache

# Clean up all containers and volumes
docker-compose down -v
docker system prune -a
```

## File Structure

```
.
├── Dockerfile                  # Multi-stage production Dockerfile
├── docker-compose.yml          # Production orchestration
├── docker-compose.dev.yml      # Development overrides
├── .dockerignore               # Build context exclusions
└── README.md                   # This file
```
