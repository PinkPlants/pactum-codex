# Issues — Pactum Backend

> Record problems, blockers, gotchas encountered during implementation.

---


## [2026-02-27T01:07] Wave 1 Parallel Execution — Persistent Timeout Issues

### Problem
All Wave 1 tasks (2-7) delegated in parallel. 4 out of 6 tasks timed out after 10 minutes despite simple implementation requirements.

**Successful**: 
- Task 6: Router (6m 47s) ✅
- Task 7: Solana types (6m 46s) ✅

**Failed (timeout)**:
- Task 2: Error types
- Task 3: Config
- Task 4: Migrations  
- Task 5: AppState

### Root Cause
Each subagent session runs `cargo check` which:
1. Downloads crates.io index (network latency: 5+ minutes)
2. Compiles ALL dependencies from scratch (ring, solana-sdk, axum: 5+ minutes)
3. Total: 10+ minutes BEFORE implementation even starts

With 600s (10min) timeout, compilation exhausts the budget leaving NO time for actual work.

### Environmental Factors
- **Network**: Persistent crates.io index update timeouts
- **No cache**: Each subagent session starts with cold cargo cache
- **Heavy deps**: solana-sdk, ring, axum with many transitive dependencies

### Attempted Solutions
1. ❌ Pre-compilation step with 15min timeout → timed out
2. ❌ Parallel task delegation → 4/6 timed out

### Recommended Mitigation
- **Sequential execution**: Run tasks one-by-one to reuse compiled artifacts
- **Longer timeouts**: Increase to 1200s (20min) for first task per wave
- **Incremental builds**: Each subsequent task in same environment uses cached artifacts

