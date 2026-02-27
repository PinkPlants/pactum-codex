## [2026-02-26T00:27:00Z] TDD Patterns for Axum 0.8 + SQLx 0.8

### 1. UNIT TEST PATTERN: Axum Handlers

**Evidence** ([Axum testing example](https://github.com/tokio-rs/axum/blob/783c83dded7ab7a598b25035583c7b5133169ff4/examples/testing/src/main.rs#L54-L82)):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{self, Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::{Service, ServiceExt}; // for `call`, `oneshot`, and `ready`

    #[tokio::test]
    async fn hello_world() {
        let app = app();

        // `Router` implements `tower::Service<Request<Body>>` so we can
        // call it like any tower service, no need to run an HTTP server.
        let response = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"Hello, World!");
    }
}
```

**Pattern Explanation:**
- Use `tower::ServiceExt::oneshot()` for single-request tests (no need for HTTP server)
- Use `http_body_util::BodyExt` to collect body bytes
- Test directly against the Router without spawning a server

---

### 2. UNIT TEST PATTERN: JSON Handlers

**Evidence** ([Axum JSON test](https://github.com/tokio-rs/axum/blob/783c83dded7ab7a598b25035583c7b5133169ff4/examples/testing/src/main.rs#L84-L105)):

```rust
#[tokio::test]
async fn json() {
    let app = app();

    let response = app
        .oneshot(
            Request::post("/json")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(
                    serde_json::to_vec(&json!([1, 2, 3, 4])).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!({ "data": [1, 2, 3, 4] }));
}
```

**Pattern Explanation:**
- Set `Content-Type: application/json` header explicitly
- Serialize JSON with `serde_json::to_vec()` for request body
- Deserialize response body with `serde_json::from_slice()`

---

### 3. UNIT TEST PATTERN: Multiple Requests Without Clone

**Evidence** ([Axum multiple requests](https://github.com/tokio-rs/axum/blob/783c83dded7ab7a598b25035583c7b5133169ff4/examples/testing/src/main.rs#L149-L172)):

```rust
#[tokio::test]
async fn multiple_request() {
    let mut app = app().into_service();

    let request = Request::get("/").body(Body::empty()).unwrap();
    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::get("/").body(Body::empty()).unwrap();
    let response = ServiceExt::<Request<Body>>::ready(&mut app)
        .await
        .unwrap()
        .call(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

**Pattern Explanation:**
- Use `.into_service()` to convert Router to mutable Service
- Use `ServiceExt::ready().await?.call()` pattern for multiple requests
- Avoids cloning the app for each request

---

### 4. UNIT TEST PATTERN: Mocking State & Extractors

**Evidence** ([Axum MockConnectInfo](https://github.com/tokio-rs/axum/blob/783c83dded7ab7a598b25035583c7b5133169ff4/examples/testing/src/main.rs#L174-L190)):

```rust
use axum::extract::connect_info::MockConnectInfo;

#[tokio::test]
async fn with_into_make_service_with_connect_info() {
    let mut app = app()
        .layer(MockConnectInfo(SocketAddr::from(([0, 0, 0, 0], 3000))))
        .into_service();

    let request = Request::get("/requires-connect-info")
        .body(Body::empty())
        .unwrap();
    let response = app.ready().await.unwrap().call(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

**Pattern Explanation:**
- Use `MockConnectInfo` layer to inject fake `ConnectInfo` extractor data
- This avoids needing `Router::into_make_service_with_connect_info()` in tests
- Apply with `.layer(MockConnectInfo(data))`

---

### 5. INTEGRATION TEST PATTERN: SQLx Test Database (Auto-managed)

**Evidence** ([SQLx test attribute docs](https://docs.rs/sqlx/latest/sqlx/attr.test)):

```rust
use sqlx::PgPool;

#[sqlx::test]
async fn test_database_query(pool: PgPool) -> sqlx::Result<()> {
    let mut conn = pool.acquire().await?;

    let result = sqlx::query("SELECT * FROM users")
        .fetch_one(&mut conn)
        .await?;

    assert_eq!(result.get::<String, _>("email"), "test@example.com");
    
    Ok(())
}
```

**Pattern Explanation:**
- `#[sqlx::test]` automatically creates a temporary test database per test
- Accepts `Pool<DB>`, `PoolConnection<DB>`, or `PoolOptions<DB>` as parameter
- Database is cleaned up automatically after test completes
- Supports Postgres, MySQL, SQLite

---

### 6. INTEGRATION TEST PATTERN: SQLx with Migrations

**Evidence** ([SQLx migrations in tests](https://docs.rs/sqlx/latest/sqlx/attr.test)):

```rust
use sqlx::PgPool;

// Option 1: Specify migration directory
#[sqlx::test(migrations = "db/migrations")]
async fn test_with_migrations(pool: PgPool) -> sqlx::Result<()> {
    let mut conn = pool.acquire().await?;
    
    let user = sqlx::query("SELECT * FROM users WHERE id = $1")
        .bind(1)
        .fetch_one(&mut conn)
        .await?;
    
    Ok(())
}

// Option 2: Reference a Migrator constant
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("db/migrations");

#[sqlx::test(migrator = "crate::MIGRATOR")]
async fn test_with_migrator(pool: PgPool) -> sqlx::Result<()> {
    // Migrations already applied
    Ok(())
}

// Option 3: Disable migrations (manual schema setup)
#[sqlx::test(migrations = false)]
async fn test_manual_schema(pool: PgPool) -> sqlx::Result<()> {
    let mut conn = pool.acquire().await?;
    
    conn.execute("CREATE TABLE test_table (id SERIAL PRIMARY KEY)").await?;
    
    Ok(())
}
```

**Pattern Explanation:**
- Default: Runs migrations from `./migrations` directory
- `migrations = "path"`: Custom migration path
- `migrator = "path::MIGRATOR"`: Reference a static Migrator
- `migrations = false`: Skip migrations for manual setup

---

### 7. INTEGRATION TEST PATTERN: SQLx with Fixtures

**Evidence** ([SQLx fixtures](https://docs.rs/sqlx/latest/sqlx/attr.test)):

```rust
use sqlx::PgPool;

// Fixtures are SQL scripts that seed test data
#[sqlx::test(fixtures("users", "posts"))]
async fn test_with_fixtures(pool: PgPool) -> sqlx::Result<()> {
    // Data from fixtures/users.sql and fixtures/posts.sql is loaded
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await?;
    
    assert!(user_count > 0);
    
    Ok(())
}

// Option 2: Specify full paths
#[sqlx::test(fixtures("./fixtures/users.sql", "./fixtures/posts.sql"))]
async fn test_with_fixture_paths(pool: PgPool) -> sqlx::Result<()> {
    Ok(())
}

// Option 3: Specify directory + script names
#[sqlx::test(fixtures(path = "./fixtures", scripts("users", "posts")))]
async fn test_with_fixture_dir(pool: PgPool) -> sqlx::Result<()> {
    Ok(())
}
```

**Pattern Explanation:**
- Fixtures are `.sql` files that insert test data
- Default location: `./fixtures/<name>.sql`
- Executed after migrations
- Can compose multiple fixtures for complex test scenarios

---

### 8. MOCK PATTERN: External Services with Mockall

**Evidence** ([Mockall automock pattern](https://github.com/FuelLabs/fuel-core/blob/master/crates/services/consensus_module/poa/src/ports.rs#L30)):

```rust
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait SolanaRpcClient: Send + Sync {
    async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64>;
    async fn send_transaction(&self, tx: &Transaction) -> Result<Signature>;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_handler_with_mock_rpc() {
        let mut mock_rpc = MockSolanaRpcClient::new();
        
        // Setup expectations
        mock_rpc
            .expect_get_balance()
            .with(predicate::eq(test_pubkey()))
            .returning(|_| Ok(1_000_000));
        
        // Inject mock into handler
        let app = create_app_with_rpc(Arc::new(mock_rpc));
        
        let response = app
            .oneshot(Request::get("/balance").body(Body::empty()).unwrap())
            .await
            .unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

**Pattern Explanation:**
- Use `#[cfg_attr(test, automock)]` on trait definitions
- In tests, use `Mock{TraitName}::new()` to create mock
- Setup expectations with `.expect_method().with().returning()`
- Inject mock via Arc into application state

---

### 9. TEST UTILITIES: JWT Token Generation

**Pattern for Testing Protected Routes:**

```rust
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub fn create_test_jwt(user_id: &str) -> String {
    let claims = Claims {
        sub: user_id.to_owned(),
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("test_secret".as_ref()),
    )
    .unwrap()
}

#[tokio::test]
async fn test_protected_route() {
    let app = app();
    let token = create_test_jwt("user_123");
    
    let response = app
        .oneshot(
            Request::get("/protected")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}
```

---

### 10. TEST UTILITIES: Database Fixtures Builder

**Pattern for Reusable Test Data:**

```rust
pub struct TestFixtures {
    pool: PgPool,
}

impl TestFixtures {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_user(&self, email: &str) -> Result<i32> {
        let user_id = sqlx::query_scalar(
            "INSERT INTO users (email, created_at) VALUES ($1, NOW()) RETURNING id"
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(user_id)
    }
    
    pub async fn create_escrow(&self, user_id: i32, amount: i64) -> Result<i32> {
        let escrow_id = sqlx::query_scalar(
            "INSERT INTO escrows (user_id, amount, status) VALUES ($1, $2, 'pending') RETURNING id"
        )
        .bind(user_id)
        .bind(amount)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(escrow_id)
    }
}

#[sqlx::test(migrations = "db/migrations")]
async fn test_with_fixtures(pool: PgPool) -> sqlx::Result<()> {
    let fixtures = TestFixtures::new(pool.clone());
    
    let user_id = fixtures.create_user("test@example.com").await?;
    let escrow_id = fixtures.create_escrow(user_id, 1000).await?;
    
    // Test your logic
    assert!(escrow_id > 0);
    
    Ok(())
}
```

---

### 11. INTEGRATION TEST PATTERN: Full Server Testing

**Evidence** ([Axum real server test](https://github.com/tokio-rs/axum/blob/783c83dded7ab7a598b25035583c7b5133169ff4/examples/testing/src/main.rs#L121-L147)):

```rust
use tokio::net::TcpListener;
use hyper_util::client::legacy::Client;

#[tokio::test]
async fn test_with_real_server() {
    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app()).await;
    });

    let client = Client::builder(hyper_util::rt::TokioExecutor::new())
        .build_http();

    let response = client
        .request(
            Request::get(format!("http://{addr}"))
                .header("Host", "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"Hello, World!");
}
```

**Pattern Explanation:**
- Bind to `0.0.0.0:0` to get random available port
- Spawn server in background with `tokio::spawn`
- Use `hyper_util::client` to make real HTTP requests
- Useful for testing middleware, TLS, WebSockets

---

### 12. BEST PRACTICES SUMMARY

**Test Organization:**
```rust
// src/handlers/auth.rs
pub async fn login(/* ... */) -> Result<Json<LoginResponse>> {
    // Implementation
}

#[cfg(test)]
mod tests {
    use super::*;
    // Unit tests for individual handlers
}

// tests/integration/auth_tests.rs
#[sqlx::test]
async fn test_login_flow(pool: PgPool) {
    // Integration tests with real database
}
```

**Dependencies for Testing:**
```toml
[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
mockall = "0.13"
jsonwebtoken = "9.3"
```

**Key Principles:**
1. **Unit tests**: Use `tower::ServiceExt::oneshot()` - no HTTP server needed
2. **Integration tests**: Use `#[sqlx::test]` for automatic database management
3. **Mocking**: Use `mockall::automock` for external services
4. **Fixtures**: Use SQLx fixtures or builder pattern for test data
5. **State injection**: Mock state via `.layer(MockConnectInfo())` or custom layers

---

### 13. ADVANCED: Testing with Shared App State

**Pattern for Testing Handlers with Database Pool:**

```rust
use axum::extract::State;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub rpc_client: Arc<dyn SolanaRpcClient>,
}

pub async fn get_balance(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
) -> Result<Json<BalanceResponse>> {
    let balance = state.rpc_client.get_balance(&pubkey).await?;
    Ok(Json(BalanceResponse { balance }))
}

#[sqlx::test]
async fn test_get_balance_handler(pool: PgPool) -> sqlx::Result<()> {
    let mut mock_rpc = MockSolanaRpcClient::new();
    mock_rpc
        .expect_get_balance()
        .returning(|_| Ok(5_000_000));
    
    let state = AppState {
        pool,
        rpc_client: Arc::new(mock_rpc),
    };
    
    let app = Router::new()
        .route("/balance/:pubkey", get(get_balance))
        .with_state(state);
    
    let response = app
        .oneshot(
            Request::get("/balance/test123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: BalanceResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.balance, 5_000_000);
    
    Ok(())
}
```

---

### References

- [Axum Testing Example](https://github.com/tokio-rs/axum/blob/783c83dded7ab7a598b25035583c7b5133169ff4/examples/testing/src/main.rs)
- [SQLx Test Attribute Documentation](https://docs.rs/sqlx/latest/sqlx/attr.test)
- [Axum Documentation](https://docs.rs/axum/0.8/axum/)
- [Tower ServiceExt](https://docs.rs/tower/latest/tower/trait.ServiceExt.html)
- [Mockall Documentation](https://docs.rs/mockall/latest/mockall/)

