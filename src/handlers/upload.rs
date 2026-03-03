use crate::config::Config;
use crate::error::AppError;
use crate::middleware::wallet_guard::AuthUserWithWallet;
use crate::services::hash::verify_client_hash;
use crate::services::storage::upload_document;
use axum::{
    extract::{Multipart, State},
    Json,
};
use serde::Serialize;

const MAX_FILE_SIZE_BYTES: usize = 50 * 1024 * 1024;
const ALLOWED_MIME_TYPES: &[&str] = &["application/pdf", "image/png", "image/jpeg"];

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct UploadResponse {
    pub storage_uri: String,
    pub content_hash: String,
    pub storage_backend: String,
}

pub async fn upload_handler(
    State(config): State<Config>,
    _auth: AuthUserWithWallet,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;
    let mut client_hash: Option<String> = None;
    let mut backend: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::UploadFailed)?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        match field_name.as_str() {
            "file" => {
                let field_content_type = field
                    .content_type()
                    .ok_or(AppError::MissingContentType)?
                    .to_string();

                if !ALLOWED_MIME_TYPES.contains(&field_content_type.as_str()) {
                    return Err(AppError::InvalidFileType);
                }

                let bytes = field.bytes().await.map_err(|_| AppError::UploadFailed)?;

                if bytes.len() > MAX_FILE_SIZE_BYTES {
                    return Err(AppError::FileTooLarge);
                }

                file_bytes = Some(bytes.to_vec());
                content_type = Some(field_content_type);
            }
            "client_hash" => {
                let value = field.text().await.map_err(|_| AppError::UploadFailed)?;
                client_hash = Some(value);
            }
            "backend" => {
                let value = field.text().await.map_err(|_| AppError::UploadFailed)?;
                backend = Some(value);
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes.ok_or(AppError::UploadFailed)?;
    let content_type = content_type.ok_or(AppError::MissingContentType)?;
    let client_hash = client_hash.ok_or(AppError::InvalidHash)?;
    let backend = backend.ok_or(AppError::UploadFailed)?;

    let response = validate_and_upload(
        &file_bytes,
        &content_type,
        &client_hash,
        &backend,
        |bytes| upload_document(&backend, bytes, &config),
    )?;

    Ok(Json(response))
}

fn validate_and_upload<U>(
    file_bytes: &[u8],
    content_type: &str,
    client_hash: &str,
    backend: &str,
    upload_fn: U,
) -> Result<UploadResponse, AppError>
where
    U: FnOnce(&[u8]) -> Result<String, AppError>,
{
    validate_and_upload_with_size_limit(
        file_bytes,
        content_type,
        client_hash,
        backend,
        MAX_FILE_SIZE_BYTES,
        upload_fn,
    )
}

fn validate_and_upload_with_size_limit<U>(
    file_bytes: &[u8],
    content_type: &str,
    client_hash: &str,
    backend: &str,
    max_file_size_bytes: usize,
    upload_fn: U,
) -> Result<UploadResponse, AppError>
where
    U: FnOnce(&[u8]) -> Result<String, AppError>,
{
    if file_bytes.len() > max_file_size_bytes {
        return Err(AppError::FileTooLarge);
    }

    if !ALLOWED_MIME_TYPES.contains(&content_type) {
        return Err(AppError::InvalidFileType);
    }

    let storage_backend = backend.to_ascii_lowercase();
    if storage_backend != "ipfs" && storage_backend != "arweave" {
        return Err(AppError::UploadFailed);
    }

    let server_hash = verify_client_hash(file_bytes, client_hash)?;
    let storage_uri = upload_fn(file_bytes).map_err(|_| AppError::UploadFailed)?;

    Ok(UploadResponse {
        storage_uri,
        content_hash: hex::encode(server_hash),
        storage_backend,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_and_upload_valid_path_returns_expected_response() {
        let file_bytes = b"pactum";
        let content_type = "application/pdf";
        let backend = "ipfs";
        let client_hash = hex::encode(crate::services::hash::compute_sha256(file_bytes));

        let result =
            validate_and_upload(file_bytes, content_type, &client_hash, backend, |_bytes| {
                Ok("ipfs://QmUploadCid123".to_string())
            });

        assert!(result.is_ok());
        let response = result.expect("expected successful upload response");
        assert_eq!(response.storage_uri, "ipfs://QmUploadCid123");
        assert_eq!(response.storage_backend, "ipfs");
        assert_eq!(response.content_hash, client_hash);
    }

    #[test]
    fn test_validate_and_upload_hash_mismatch_returns_hash_mismatch() {
        let file_bytes = b"pactum";
        let result = validate_and_upload(
            file_bytes,
            "application/pdf",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "ipfs",
            |_bytes| Ok("ipfs://QmUploadCid123".to_string()),
        );

        assert!(matches!(result, Err(AppError::HashMismatch)));
    }

    #[test]
    fn test_validate_and_upload_invalid_mime_returns_invalid_file_type() {
        let file_bytes = b"pactum";
        let client_hash = hex::encode(crate::services::hash::compute_sha256(file_bytes));

        let result =
            validate_and_upload(file_bytes, "text/plain", &client_hash, "ipfs", |_bytes| {
                Ok("ipfs://QmUploadCid123".to_string())
            });

        assert!(matches!(result, Err(AppError::InvalidFileType)));
    }

    #[test]
    fn test_validate_and_upload_file_too_large_returns_file_too_large() {
        let file_bytes = [1u8, 2u8];
        let client_hash = hex::encode(crate::services::hash::compute_sha256(&file_bytes));

        let result = validate_and_upload_with_size_limit(
            &file_bytes,
            "application/pdf",
            &client_hash,
            "ipfs",
            1,
            |_bytes| Ok("ipfs://QmUploadCid123".to_string()),
        );

        assert!(matches!(result, Err(AppError::FileTooLarge)));
    }
}
