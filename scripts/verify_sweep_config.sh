#!/bin/bash
# Sweep Config Migration Verification Script
# Verifies that sweep_config table exists and is queryable after migration

set -e

EVIDENCE_DIR=".sisyphus/evidence"
mkdir -p "$EVIDENCE_DIR"

echo "=== Sweep Config Migration Verification ==="
echo ""

# Check if containers are running
if ! docker compose ps | grep -q "pactum-postgres"; then
    echo "ERROR: PostgreSQL container not running"
    echo "Run: docker compose up -d postgres"
    exit 1
fi

if ! docker compose ps | grep -q "pactum-api"; then
    echo "ERROR: API container not running"
    echo "Run: docker compose up -d api"
    exit 1
fi

echo "✓ Containers are running"

# Check migration ledger
echo ""
echo "=== Checking Migration Ledger ==="
docker exec pactum-postgres psql -U pactum -d pactum -c "SELECT version, success FROM _sqlx_migrations ORDER BY version;" 2>/dev/null || {
    echo "ERROR: Failed to query _sqlx_migrations"
    exit 1
}

# Check sweep_config table exists
echo ""
echo "=== Checking sweep_config Table ==="
if ! docker exec pactum-postgres psql -U pactum -d pactum -c "SELECT id, last_sweep_at FROM sweep_config;" 2>/dev/null; then
    echo "ERROR: sweep_config table not found or not queryable"
    exit 1
fi

echo "✓ sweep_config table exists and is queryable"

# Check the specific query used by keeper
echo ""
echo "=== Checking Keeper Query ==="
if ! docker exec pactum-postgres psql -U pactum -d pactum -c "SELECT NOT EXISTS(SELECT 1 FROM sweep_config WHERE last_sweep_at > extract(epoch from now()) - 86400);" 2>/dev/null; then
    echo "ERROR: Keeper query failed"
    exit 1
fi

echo "✓ Keeper query succeeds"

# Check for relation errors in logs
echo ""
echo "=== Checking Logs for Errors ==="
if docker compose logs api --since=5m 2>/dev/null | grep -q 'relation "sweep_config" does not exist'; then
    echo "ERROR: Found 'relation sweep_config does not exist' in recent logs"
    exit 1
fi

echo "✓ No sweep_config relation errors in recent logs"

# Save evidence
echo ""
echo "=== Saving Evidence ==="
{
    echo "Verification Date: $(date)"
    echo ""
    echo "Migration Ledger:"
    docker exec pactum-postgres psql -U pactum -d pactum -c "SELECT version, success FROM _sqlx_migrations ORDER BY version;" 2>/dev/null
    echo ""
    echo "sweep_config Table:"
    docker exec pactum-postgres psql -U pactum -d pactum -c "SELECT id, last_sweep_at FROM sweep_config;" 2>/dev/null
    echo ""
    echo "Recent Keeper Log Entries:"
    docker compose logs api --since=5m 2>/dev/null | grep -i "sweep_config\|keeper.*sweep" | tail -5 || echo "No recent sweep entries"
} > "$EVIDENCE_DIR/verification-$(date +%Y%m%d-%H%M%S).txt"

echo ""
echo "=== VERIFICATION PASSED ==="
echo "All checks successful. sweep_config migration is working correctly."
exit 0
