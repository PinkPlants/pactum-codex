use crate::{config::Config, error::AppError};
use reqwest::{multipart, Client};
use serde_json::Value;
use std::fs;

const ARWEAVE_UPLOAD_URL: &str = "https://arweave.net/tx";

trait StorageHttpClient {
    fn post_ipfs(&self, url: &str, bearer_token: &str, data: &[u8]) -> Result<Value, AppError>;
    fn post_arweave(&self, url: &str, wallet_json: &[u8], data: &[u8]) -> Result<Value, AppError>;
}

struct ReqwestStorageHttpClient;

impl ReqwestStorageHttpClient {
    fn run_async<F, T>(future: F) -> Result<T, AppError>
    where
        F: std::future::Future<Output = Result<T, AppError>>,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(future))
        } else {
            let runtime = tokio::runtime::Runtime::new().map_err(|_| AppError::UploadFailed)?;
            runtime.block_on(future)
        }
    }
}

impl StorageHttpClient for ReqwestStorageHttpClient {
    fn post_ipfs(&self, url: &str, bearer_token: &str, data: &[u8]) -> Result<Value, AppError> {
        Self::run_async(async move {
            let client = Client::new();
            let part = multipart::Part::bytes(data.to_vec()).file_name("document.bin");
            let form = multipart::Form::new().part("file", part);

            let request = client.post(url).bearer_auth(bearer_token).multipart(form);

            let response = request.send().await.map_err(|_| AppError::UploadFailed)?;
            if !response.status().is_success() {
                return Err(AppError::UploadFailed);
            }

            response
                .json::<Value>()
                .await
                .map_err(|_| AppError::UploadFailed)
        })
    }

    fn post_arweave(&self, url: &str, wallet_json: &[u8], data: &[u8]) -> Result<Value, AppError> {
        let wallet_bytes = wallet_json.to_vec();
        let file_bytes = data.to_vec();
        Self::run_async(async move {
            let client = Client::new();
            let file_part = multipart::Part::bytes(file_bytes).file_name("document.bin");
            let wallet_part = multipart::Part::bytes(wallet_bytes)
                .file_name("wallet.json")
                .mime_str("application/json")
                .map_err(|_| AppError::UploadFailed)?;
            let form = multipart::Form::new()
                .part("file", file_part)
                .part("wallet", wallet_part);
            let request = client.post(url).multipart(form);

            let response = request.send().await.map_err(|_| AppError::UploadFailed)?;
            if !response.status().is_success() {
                return Err(AppError::UploadFailed);
            }

            response
                .json::<Value>()
                .await
                .map_err(|_| AppError::UploadFailed)
        })
    }
}

fn extract_uri_id(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| payload.get(key).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn upload_to_ipfs_with_client(
    data: &[u8],
    config: &Config,
    client: &dyn StorageHttpClient,
) -> Result<String, AppError> {
    let endpoint = "https://uploads.pinata.cloud/v3/files";
    let payload = client.post_ipfs(endpoint, &config.pinata_jwt, data)?;

    let cid =
        extract_uri_id(&payload, &["IpfsHash", "cid", "Hash"]).ok_or(AppError::UploadFailed)?;
    Ok(format!("ipfs://{cid}"))
}

fn upload_to_arweave_with_client(
    data: &[u8],
    config: &Config,
    client: &dyn StorageHttpClient,
) -> Result<String, AppError> {
    let wallet_json = fs::read(&config.arweave_wallet_path).map_err(|_| AppError::InternalError)?;
    let payload = client.post_arweave(ARWEAVE_UPLOAD_URL, &wallet_json, data)?;
    let tx_id = extract_uri_id(&payload, &["id", "tx_id", "transactionId"])
        .ok_or(AppError::UploadFailed)?;
    Ok(format!("ar://{tx_id}"))
}

fn upload_document_with_client(
    backend: &str,
    data: &[u8],
    config: &Config,
    client: &dyn StorageHttpClient,
) -> Result<String, AppError> {
    match backend.to_ascii_lowercase().as_str() {
        "ipfs" => upload_to_ipfs_with_client(data, config, client),
        "arweave" => upload_to_arweave_with_client(data, config, client),
        _ => Err(AppError::PaymentMethodUnsupported),
    }
}

pub fn upload_to_ipfs(data: &[u8], config: &Config) -> Result<String, AppError> {
    upload_to_ipfs_with_client(data, config, &ReqwestStorageHttpClient)
}

pub fn upload_to_arweave(data: &[u8], config: &Config) -> Result<String, AppError> {
    upload_to_arweave_with_client(data, config, &ReqwestStorageHttpClient)
}

pub fn upload_document(backend: &str, data: &[u8], config: &Config) -> Result<String, AppError> {
    upload_document_with_client(backend, data, config, &ReqwestStorageHttpClient)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{StablecoinInfo, StablecoinRegistry};
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct MockStorageHttpClient {
        ipfs_calls: Mutex<u32>,
        arweave_calls: Mutex<u32>,
        ipfs_responses: Mutex<VecDeque<Result<Value, AppError>>>,
        arweave_responses: Mutex<VecDeque<Result<Value, AppError>>>,
        arweave_wallet_sizes: Mutex<Vec<usize>>,
    }

    impl MockStorageHttpClient {
        fn new(
            ipfs_responses: Vec<Result<Value, AppError>>,
            arweave_responses: Vec<Result<Value, AppError>>,
        ) -> Self {
            Self {
                ipfs_calls: Mutex::new(0),
                arweave_calls: Mutex::new(0),
                ipfs_responses: Mutex::new(VecDeque::from(ipfs_responses)),
                arweave_responses: Mutex::new(VecDeque::from(arweave_responses)),
                arweave_wallet_sizes: Mutex::new(Vec::new()),
            }
        }

        fn ipfs_call_count(&self) -> u32 {
            *self.ipfs_calls.lock().expect("lock poisoned")
        }

        fn arweave_call_count(&self) -> u32 {
            *self.arweave_calls.lock().expect("lock poisoned")
        }

        fn latest_arweave_wallet_size(&self) -> Option<usize> {
            self.arweave_wallet_sizes
                .lock()
                .expect("lock poisoned")
                .last()
                .copied()
        }
    }

    impl StorageHttpClient for MockStorageHttpClient {
        fn post_ipfs(
            &self,
            _url: &str,
            _bearer_token: &str,
            _data: &[u8],
        ) -> Result<Value, AppError> {
            *self.ipfs_calls.lock().expect("lock poisoned") += 1;
            self.ipfs_responses
                .lock()
                .expect("lock poisoned")
                .pop_front()
                .expect("expected mock ipfs response")
        }

        fn post_arweave(
            &self,
            _url: &str,
            wallet_json: &[u8],
            _data: &[u8],
        ) -> Result<Value, AppError> {
            *self.arweave_calls.lock().expect("lock poisoned") += 1;
            self.arweave_wallet_sizes
                .lock()
                .expect("lock poisoned")
                .push(wallet_json.len());
            self.arweave_responses
                .lock()
                .expect("lock poisoned")
                .pop_front()
                .expect("expected mock arweave response")
        }
    }

    fn create_wallet_file(contents: &[u8]) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("pactum-storage-wallet-{unique}.json"));
        fs::write(&path, contents).expect("wallet file should be writable");
        path
    }

    fn test_config(arweave_wallet_path: PathBuf) -> Config {
        Config {
            database_url: "postgres://localhost/test".to_string(),
            solana_rpc_url: "https://api.devnet.solana.com".to_string(),
            solana_ws_url: "wss://api.devnet.solana.com".to_string(),
            program_id: "DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P".to_string(),
            jwt_secret: "secret".to_string(),
            jwt_access_expiry_seconds: 900,
            jwt_refresh_expiry_seconds: 604800,
            encryption_key: "encryption_key".to_string(),
            encryption_index_key: "encryption_index_key".to_string(),
            google_client_id: "google_client_id".to_string(),
            google_client_secret: "google_client_secret".to_string(),
            google_redirect_uri: "https://api.pactum.app/auth/oauth/google/callback".to_string(),
            microsoft_client_id: "microsoft_client_id".to_string(),
            microsoft_client_secret: "microsoft_client_secret".to_string(),
            microsoft_redirect_uri: "https://api.pactum.app/auth/oauth/microsoft/callback"
                .to_string(),
            microsoft_tenant: "common".to_string(),
            resend_api_key: "resend_api_key".to_string(),
            email_from: "noreply@pactum.app".to_string(),
            invite_base_url: "https://app.pactum.app/invite".to_string(),
            invite_expiry_seconds: 604800,
            invite_reminder_after_seconds: 259200,
            platform_fee_usd_cents: 199,
            platform_fee_free_tier: 3,
            platform_nonrefundable_fee_cents: 10,
            platform_vault_pubkey: "vault_pubkey".to_string(),
            platform_vault_keypair_path: PathBuf::from("/tmp/vault.json"),
            platform_treasury_pubkey: "treasury_pubkey".to_string(),
            platform_treasury_keypair_path: PathBuf::from("/tmp/treasury.json"),
            vault_min_sol_alert: 0.5,
            vault_min_sol_circuit_breaker: 0.1,
            vault_funding_rate_limit_per_hour: 50,
            treasury_min_usdc_alert: 20_000_000,
            treasury_float_per_token: 50_000_000,
            treasury_sweep_dest: "sweep_dest".to_string(),
            stablecoin_registry: StablecoinRegistry {
                usdc: StablecoinInfo {
                    symbol: "usdc",
                    mint: "usdc_mint".to_string(),
                    ata: "usdc_ata".to_string(),
                    decimals: 6,
                },
                usdt: StablecoinInfo {
                    symbol: "usdt",
                    mint: "usdt_mint".to_string(),
                    ata: "usdt_ata".to_string(),
                    decimals: 6,
                },
                pyusd: StablecoinInfo {
                    symbol: "pyusd",
                    mint: "pyusd_mint".to_string(),
                    ata: "pyusd_ata".to_string(),
                    decimals: 6,
                },
            },
            pinata_jwt: "pinata_jwt".to_string(),
            pinata_gateway_domain: "gateway.pinata.cloud".to_string(),
            arweave_wallet_path,
            server_port: 8080,
            server_host: "0.0.0.0".to_string(),
        }
    }

    #[test]
    fn test_upload_document_dispatches_to_ipfs() {
        let wallet_path = create_wallet_file(br#"{"kty":"RSA"}"#);
        let config = test_config(wallet_path.clone());
        let mock = MockStorageHttpClient::new(
            vec![Ok(serde_json::json!({ "IpfsHash": "bafy123" }))],
            vec![],
        );

        let result = upload_document_with_client("ipfs", b"hello", &config, &mock);
        let _ = fs::remove_file(wallet_path);

        assert!(result.is_ok());
        assert_eq!(result.expect("expected success"), "ipfs://bafy123");
        assert_eq!(mock.ipfs_call_count(), 1);
        assert_eq!(mock.arweave_call_count(), 0);
    }

    #[test]
    fn test_upload_document_dispatches_to_arweave() {
        let wallet_content = br#"{"kty":"RSA","n":"abc"}"#;
        let wallet_path = create_wallet_file(wallet_content);
        let config = test_config(wallet_path.clone());
        let mock =
            MockStorageHttpClient::new(vec![], vec![Ok(serde_json::json!({ "id": "ar_tx_123" }))]);

        let result = upload_document_with_client("arweave", b"hello", &config, &mock);
        let _ = fs::remove_file(wallet_path);

        assert!(result.is_ok());
        assert_eq!(result.expect("expected success"), "ar://ar_tx_123");
        assert_eq!(mock.ipfs_call_count(), 0);
        assert_eq!(mock.arweave_call_count(), 1);
        assert_eq!(
            mock.latest_arweave_wallet_size(),
            Some(wallet_content.len())
        );
    }

    #[test]
    fn test_upload_document_unknown_backend_returns_error() {
        let wallet_path = create_wallet_file(br#"{"kty":"RSA"}"#);
        let config = test_config(wallet_path.clone());
        let mock = MockStorageHttpClient::new(vec![], vec![]);

        let result = upload_document_with_client("unknown", b"hello", &config, &mock);
        let _ = fs::remove_file(wallet_path);

        assert!(matches!(result, Err(AppError::PaymentMethodUnsupported)));
    }

    #[test]
    fn test_upload_to_ipfs_http_error_returns_upload_failed() {
        let wallet_path = create_wallet_file(br#"{"kty":"RSA"}"#);
        let config = test_config(wallet_path.clone());
        let mock = MockStorageHttpClient::new(vec![Err(AppError::UploadFailed)], vec![]);

        let result = upload_to_ipfs_with_client(b"hello", &config, &mock);
        let _ = fs::remove_file(wallet_path);

        assert!(matches!(result, Err(AppError::UploadFailed)));
        assert_eq!(mock.ipfs_call_count(), 1);
    }

    #[test]
    fn test_upload_to_arweave_http_error_returns_upload_failed() {
        let wallet_path = create_wallet_file(br#"{"kty":"RSA"}"#);
        let config = test_config(wallet_path.clone());
        let mock = MockStorageHttpClient::new(vec![], vec![Err(AppError::UploadFailed)]);

        let result = upload_to_arweave_with_client(b"hello", &config, &mock);
        let _ = fs::remove_file(wallet_path);

        assert!(matches!(result, Err(AppError::UploadFailed)));
        assert_eq!(mock.arweave_call_count(), 1);
    }

    #[test]
    fn test_upload_to_arweave_wallet_read_error_returns_internal_error() {
        let missing_wallet_path = std::env::temp_dir().join("pactum-storage-wallet-missing.json");
        let config = test_config(missing_wallet_path);
        let mock = MockStorageHttpClient::new(vec![], vec![]);

        let result = upload_to_arweave_with_client(b"hello", &config, &mock);

        assert!(matches!(result, Err(AppError::InternalError)));
        assert_eq!(mock.arweave_call_count(), 0);
    }
}
