use crate::state::AppState;
use async_trait::async_trait;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

use std::path::PathBuf;

/// Stablecoin information struct
#[derive(Clone)]
pub struct StablecoinInfo {
    pub symbol: &'static str,
    pub mint: String,
    pub ata: String,
    pub decimals: u8,
}

/// Registry of all supported stablecoins
#[derive(Clone)]
pub struct StablecoinRegistry {
    pub usdc: StablecoinInfo,
    pub usdt: StablecoinInfo,
    pub pyusd: StablecoinInfo,
}

impl StablecoinRegistry {
    /// Resolve a payment method string to its StablecoinInfo.
    /// Returns None for unknown or unsupported tokens.
    pub fn resolve(&self, method: &str) -> Option<&StablecoinInfo> {
        match method {
            "usdc" => Some(&self.usdc),
            "usdt" => Some(&self.usdt),
            "pyusd" => Some(&self.pyusd),
            _ => None,
        }
    }
}

/// Main application configuration loaded from environment variables
#[derive(Clone)]
pub struct Config {
    // ===== DATABASE =====
    pub database_url: String,

    // ===== SOLANA =====
    pub solana_rpc_url: String,
    pub solana_ws_url: String,
    pub program_id: String,

    // ===== JWT =====
    pub jwt_secret: String,
    pub jwt_access_expiry_seconds: u64,
    pub jwt_refresh_expiry_seconds: u64,

    // ===== ENCRYPTION =====
    pub encryption_key: String,
    pub encryption_index_key: String,

    // ===== OAUTH =====
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_redirect_uri: String,

    pub microsoft_client_id: String,
    pub microsoft_client_secret: String,
    pub microsoft_redirect_uri: String,
    pub microsoft_tenant: String,

    // ===== EMAIL =====
    pub resend_api_key: String,
    pub email_from: String,
    pub invite_base_url: String,
    pub invite_expiry_seconds: u64,
    pub invite_reminder_after_seconds: u64,

    // ===== PAYMENT =====
    pub platform_fee_usd_cents: u32,
    pub platform_fee_free_tier: u32,
    pub platform_nonrefundable_fee_cents: u32,

    // ===== HOT WALLET KEYPAIRS =====
    pub platform_vault_pubkey: String,
    pub platform_vault_keypair_path: PathBuf,

    pub platform_treasury_pubkey: String,
    pub platform_treasury_keypair_path: PathBuf,

    // ===== HOT WALLET SAFETY THRESHOLDS =====
    pub vault_min_sol_alert: f64,
    pub vault_min_sol_circuit_breaker: f64,
    pub vault_funding_rate_limit_per_hour: u32,
    pub treasury_min_usdc_alert: u64,
    pub treasury_float_per_token: u64,
    pub treasury_sweep_dest: String,

    // ===== STABLECOINS =====
    pub stablecoin_registry: StablecoinRegistry,

    // ===== STORAGE =====
    pub pinata_jwt: String,
    pub pinata_gateway_domain: String,
    pub arweave_wallet_path: PathBuf,

    // ===== SERVER =====
    pub server_port: u16,
    pub server_host: String,
}

impl Config {
    /// Load configuration from environment variables
    /// Panics if any required variable is missing or invalid
    pub fn from_env() -> Self {
        // Load .env file if present
        dotenvy::dotenv().ok();

        // When PGPASSWORD_FILE is set (Docker secrets), read the password from
        // that file and inject it into DATABASE_URL before connecting.
        let database_url = {
            let base_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");

            if let Ok(pw_file) = std::env::var("PGPASSWORD_FILE") {
                let password = std::fs::read_to_string(&pw_file)
                    .unwrap_or_else(|e| panic!("Cannot read PGPASSWORD_FILE '{pw_file}': {e}"))
                    .trim()
                    .to_string();

                inject_password_into_url(&base_url, &password)
            } else {
                base_url
            }
        };

        let config = Config {
            // Database
            database_url,

            // Solana
            solana_rpc_url: std::env::var("SOLANA_RPC_URL").expect("SOLANA_RPC_URL is required"),
            solana_ws_url: std::env::var("SOLANA_WS_URL").expect("SOLANA_WS_URL is required"),
            program_id: std::env::var("PROGRAM_ID").expect("PROGRAM_ID is required"),

            // JWT
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET is required (256-bit hex)"),
            jwt_access_expiry_seconds: std::env::var("JWT_ACCESS_EXPIRY_SECONDS")
                .unwrap_or_else(|_| "900".to_string())
                .parse()
                .expect("JWT_ACCESS_EXPIRY_SECONDS must be a valid u64"),
            jwt_refresh_expiry_seconds: std::env::var("JWT_REFRESH_EXPIRY_SECONDS")
                .unwrap_or_else(|_| "604800".to_string())
                .parse()
                .expect("JWT_REFRESH_EXPIRY_SECONDS must be a valid u64"),

            // Encryption
            encryption_key: std::env::var("ENCRYPTION_KEY")
                .expect("ENCRYPTION_KEY is required (256-bit hex)"),
            encryption_index_key: std::env::var("ENCRYPTION_INDEX_KEY")
                .expect("ENCRYPTION_INDEX_KEY is required (HMAC key for blind index)"),

            // OAuth
            google_client_id: std::env::var("GOOGLE_CLIENT_ID")
                .expect("GOOGLE_CLIENT_ID is required"),
            google_client_secret: std::env::var("GOOGLE_CLIENT_SECRET")
                .expect("GOOGLE_CLIENT_SECRET is required"),
            google_redirect_uri: std::env::var("GOOGLE_REDIRECT_URI").unwrap_or_else(|_| {
                "https://api.pactum.app/auth/oauth/google/callback".to_string()
            }),

            microsoft_client_id: std::env::var("MICROSOFT_CLIENT_ID")
                .expect("MICROSOFT_CLIENT_ID is required"),
            microsoft_client_secret: std::env::var("MICROSOFT_CLIENT_SECRET")
                .expect("MICROSOFT_CLIENT_SECRET is required"),
            microsoft_redirect_uri: std::env::var("MICROSOFT_REDIRECT_URI").unwrap_or_else(|_| {
                "https://api.pactum.app/auth/oauth/microsoft/callback".to_string()
            }),
            microsoft_tenant: std::env::var("MICROSOFT_TENANT")
                .unwrap_or_else(|_| "common".to_string()),

            // Email
            resend_api_key: std::env::var("RESEND_API_KEY").expect("RESEND_API_KEY is required"),
            email_from: std::env::var("EMAIL_FROM")
                .unwrap_or_else(|_| "noreply@pactum.app".to_string()),
            invite_base_url: std::env::var("INVITE_BASE_URL")
                .unwrap_or_else(|_| "https://app.pactum.app/invite".to_string()),
            invite_expiry_seconds: std::env::var("INVITE_EXPIRY_SECONDS")
                .unwrap_or_else(|_| "604800".to_string())
                .parse()
                .expect("INVITE_EXPIRY_SECONDS must be a valid u64"),
            invite_reminder_after_seconds: std::env::var("INVITE_REMINDER_AFTER_SECONDS")
                .unwrap_or_else(|_| "259200".to_string())
                .parse()
                .expect("INVITE_REMINDER_AFTER_SECONDS must be a valid u64"),

            // Payment
            platform_fee_usd_cents: std::env::var("PLATFORM_FEE_USD_CENTS")
                .unwrap_or_else(|_| "199".to_string())
                .parse()
                .expect("PLATFORM_FEE_USD_CENTS must be a valid u32"),
            platform_fee_free_tier: std::env::var("PLATFORM_FEE_FREE_TIER")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .expect("PLATFORM_FEE_FREE_TIER must be a valid u32"),
            platform_nonrefundable_fee_cents: std::env::var("PLATFORM_NONREFUNDABLE_FEE_CENTS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .expect("PLATFORM_NONREFUNDABLE_FEE_CENTS must be a valid u32"),

            // Hot wallet keypairs
            platform_vault_pubkey: std::env::var("PLATFORM_VAULT_PUBKEY")
                .expect("PLATFORM_VAULT_PUBKEY is required"),
            platform_vault_keypair_path: PathBuf::from(
                std::env::var("PLATFORM_VAULT_KEYPAIR_PATH")
                    .expect("PLATFORM_VAULT_KEYPAIR_PATH is required"),
            ),

            platform_treasury_pubkey: std::env::var("PLATFORM_TREASURY_PUBKEY")
                .expect("PLATFORM_TREASURY_PUBKEY is required"),
            platform_treasury_keypair_path: PathBuf::from(
                std::env::var("PLATFORM_TREASURY_KEYPAIR_PATH")
                    .expect("PLATFORM_TREASURY_KEYPAIR_PATH is required"),
            ),

            // Hot wallet safety thresholds
            vault_min_sol_alert: std::env::var("VAULT_MIN_SOL_ALERT")
                .unwrap_or_else(|_| "0.5".to_string())
                .parse()
                .expect("VAULT_MIN_SOL_ALERT must be a valid f64"),
            vault_min_sol_circuit_breaker: std::env::var("VAULT_MIN_SOL_CIRCUIT_BREAKER")
                .unwrap_or_else(|_| "0.1".to_string())
                .parse()
                .expect("VAULT_MIN_SOL_CIRCUIT_BREAKER must be a valid f64"),
            vault_funding_rate_limit_per_hour: std::env::var("VAULT_FUNDING_RATE_LIMIT_PER_HOUR")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .expect("VAULT_FUNDING_RATE_LIMIT_PER_HOUR must be a valid u32"),
            treasury_min_usdc_alert: std::env::var("TREASURY_MIN_USDC_ALERT")
                .unwrap_or_else(|_| "20000000".to_string())
                .parse()
                .expect("TREASURY_MIN_USDC_ALERT must be a valid u64"),
            treasury_float_per_token: std::env::var("TREASURY_FLOAT_PER_TOKEN")
                .unwrap_or_else(|_| "50000000".to_string())
                .parse()
                .expect("TREASURY_FLOAT_PER_TOKEN must be a valid u64"),
            treasury_sweep_dest: std::env::var("TREASURY_SWEEP_DEST")
                .expect("TREASURY_SWEEP_DEST is required (cold wallet or multisig address)"),

            // Stablecoins
            stablecoin_registry: StablecoinRegistry {
                usdc: StablecoinInfo {
                    symbol: "usdc",
                    mint: std::env::var("STABLECOIN_USDC_MINT").unwrap_or_else(|_| {
                        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()
                    }),
                    ata: std::env::var("STABLECOIN_USDC_ATA")
                        .expect("STABLECOIN_USDC_ATA is required"),
                    decimals: 6,
                },
                usdt: StablecoinInfo {
                    symbol: "usdt",
                    mint: std::env::var("STABLECOIN_USDT_MINT").unwrap_or_else(|_| {
                        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string()
                    }),
                    ata: std::env::var("STABLECOIN_USDT_ATA")
                        .expect("STABLECOIN_USDT_ATA is required"),
                    decimals: 6,
                },
                pyusd: StablecoinInfo {
                    symbol: "pyusd",
                    mint: std::env::var("STABLECOIN_PYUSD_MINT").unwrap_or_else(|_| {
                        "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo".to_string()
                    }),
                    ata: std::env::var("STABLECOIN_PYUSD_ATA")
                        .expect("STABLECOIN_PYUSD_ATA is required"),
                    decimals: 6,
                },
            },

            // Storage
            pinata_jwt: std::env::var("PINATA_JWT").expect("PINATA_JWT is required"),
            pinata_gateway_domain: std::env::var("PINATA_GATEWAY_DOMAIN")
                .unwrap_or_else(|_| "gateway.pinata.cloud".to_string()),
            arweave_wallet_path: PathBuf::from(
                std::env::var("ARWEAVE_WALLET_PATH")
                    .unwrap_or_else(|_| "./arweave-wallet.json".to_string()),
            ),

            // Server
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("SERVER_PORT must be a valid u16"),
            server_host: std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
        };

        config
    }
}

/// Inject a password into a PostgreSQL URL: `postgres://user@host` → `postgres://user:pw@host`
fn inject_password_into_url(url: &str, password: &str) -> String {
    let scheme_end = url.find("://").expect("DATABASE_URL must contain '://'") + 3;
    let rest = &url[scheme_end..];

    match rest.find('@') {
        Some(at_pos) => {
            let user = &rest[..at_pos];
            let after_at = &rest[at_pos..];
            format!("{}{}:{}{}", &url[..scheme_end], user, password, after_at)
        }
        None => panic!("DATABASE_URL must contain '@' (e.g. postgres://user@host/db)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stablecoin_registry_resolve() {
        let registry = StablecoinRegistry {
            usdc: StablecoinInfo {
                symbol: "usdc",
                mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                ata: "address_usdc".to_string(),
                decimals: 6,
            },
            usdt: StablecoinInfo {
                symbol: "usdt",
                mint: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(),
                ata: "address_usdt".to_string(),
                decimals: 6,
            },
            pyusd: StablecoinInfo {
                symbol: "pyusd",
                mint: "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo".to_string(),
                ata: "address_pyusd".to_string(),
                decimals: 6,
            },
        };

        assert!(registry.resolve("usdc").is_some());
        assert!(registry.resolve("usdt").is_some());
        assert!(registry.resolve("pyusd").is_some());
        assert!(registry.resolve("invalid").is_none());
        assert_eq!(registry.resolve("usdc").unwrap().symbol, "usdc");
    }

    #[test]
    fn test_inject_password_into_url() {
        assert_eq!(
            inject_password_into_url("postgres://pactum@localhost:5432/pactum", "s3cret"),
            "postgres://pactum:s3cret@localhost:5432/pactum"
        );
    }

    #[test]
    fn test_inject_password_preserves_existing_path() {
        assert_eq!(
            inject_password_into_url("postgres://user@host:5432/mydb?sslmode=require", "pw"),
            "postgres://user:pw@host:5432/mydb?sslmode=require"
        );
    }

    #[test]
    #[should_panic(expected = "must contain '@'")]
    fn test_inject_password_panics_without_at() {
        inject_password_into_url("postgres://localhost/db", "pw");
    }
}
