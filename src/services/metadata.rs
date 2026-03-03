use crate::config::Config;
use crate::error::AppError;
use crate::solana_types::AgreementStateWire;
use serde_json::{json, Value};

/// Pactum Protocol seal Arweave transaction ID (permanent storage)
pub const PACTUM_SEAL_TX_ID: &str = "pactum_seal_arweave_tx_id";

/// Build NFT metadata JSON for an agreement
///
/// Generates the standard NFT metadata structure containing:
/// - name: "Pactum #{id_short} — {title}"
/// - description: Standard agreement credential text
/// - image: Arweave link to Pactum seal
/// - animation_url: Link to stored agreement content
/// - external_url: Link to pactum.app agreement view
/// - attributes: Array of agreement metadata attributes
pub fn build_metadata_json(agreement: &AgreementStateWire, pda: &str) -> Value {
    let id_short = hex::encode(&agreement.agreement_id[..4]);

    json!({
        "name": format!("Pactum #{} — {}", id_short, agreement.title),
        "description": "On-chain agreement credential issued via Pactum Protocol.",
        "image": format!("ar://{}", PACTUM_SEAL_TX_ID),
        "animation_url": format!("ar://{}", agreement.storage_uri),
        "external_url": format!("https://pactum.app/agreement/{}", pda),
        "attributes": build_attributes(agreement)
    })
}

/// Build metadata attributes array for an agreement
///
/// Includes key agreement metadata as NFT attributes:
/// - status: Current agreement status
/// - parties: Number of signing parties
/// - storage_backend: IPFS or Arweave
/// - created_at: Unix timestamp
/// - expires_at: Unix timestamp
fn build_attributes(agreement: &AgreementStateWire) -> Vec<Value> {
    vec![
        json!({
            "trait_type": "Status",
            "value": format!("{:?}", agreement.status)
        }),
        json!({
            "trait_type": "Parties",
            "value": agreement.parties.len().to_string()
        }),
        json!({
            "trait_type": "Storage Backend",
            "value": format!("{:?}", agreement.storage_backend)
        }),
        json!({
            "trait_type": "Created At",
            "value": agreement.created_at.to_string()
        }),
        json!({
            "trait_type": "Expires At",
            "value": agreement.expires_at.to_string()
        }),
    ]
}

/// Upload metadata JSON to storage backend and return storage URI
///
/// Serializes metadata to JSON, uploads via storage service,
/// and returns the storage URI (ipfs:// or ar://).
pub fn upload_metadata_json(
    agreement: &AgreementStateWire,
    pda: &str,
    backend: &str,
    config: &Config,
) -> Result<String, AppError> {
    // Build metadata JSON
    let metadata = build_metadata_json(agreement, pda);

    // Serialize to JSON bytes
    let json_bytes = serde_json::to_vec(&metadata).map_err(|_| AppError::InternalError)?;

    // Upload to storage backend
    crate::services::storage::upload_document(backend, &json_bytes, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solana_types::{AgreementStatus, StorageBackend};

    fn create_test_agreement() -> AgreementStateWire {
        AgreementStateWire {
            creator: [0u8; 32],
            agreement_id: [
                1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            ],
            content_hash: [0u8; 32],
            title: "Test Agreement".to_string(),
            storage_uri: "ar://test_content_tx_id".to_string(),
            storage_backend: StorageBackend::Arweave,
            parties: vec![[1u8; 32], [2u8; 32]],
            signed_by: vec![[1u8; 32]],
            signed_at: vec![1000],
            status: AgreementStatus::PendingSignatures,
            created_at: 1000,
            expires_at: 2000,
            completed_at: None,
            revoked_at: None,
            nft_asset: None,
            collection: [0u8; 32],
            vault_funder: [0u8; 32],
            revoke_votes: vec![],
            revoke_retract_counts: vec![],
            bump: 0,
        }
    }

    #[test]
    fn test_build_metadata_json_contains_required_fields() {
        let agreement = create_test_agreement();
        let pda = "test_pda_address";

        let metadata = build_metadata_json(&agreement, pda);

        // Verify all required fields are present
        assert!(metadata.get("name").is_some());
        assert!(metadata.get("description").is_some());
        assert!(metadata.get("image").is_some());
        assert!(metadata.get("animation_url").is_some());
        assert!(metadata.get("external_url").is_some());
        assert!(metadata.get("attributes").is_some());
    }

    #[test]
    fn test_build_metadata_json_name_includes_id_and_title() {
        let agreement = create_test_agreement();
        let pda = "test_pda_address";

        let metadata = build_metadata_json(&agreement, pda);
        let name = metadata.get("name").unwrap().as_str().unwrap();

        // Verify name format
        assert!(name.starts_with("Pactum #"));
        assert!(name.contains("Test Agreement"));
    }

    #[test]
    fn test_build_metadata_json_image_contains_seal_tx_id() {
        let agreement = create_test_agreement();
        let pda = "test_pda_address";

        let metadata = build_metadata_json(&agreement, pda);
        let image = metadata.get("image").unwrap().as_str().unwrap();

        assert_eq!(image, format!("ar://{}", PACTUM_SEAL_TX_ID));
    }

    #[test]
    fn test_build_metadata_json_animation_url_uses_storage_uri() {
        let agreement = create_test_agreement();
        let pda = "test_pda_address";

        let metadata = build_metadata_json(&agreement, pda);
        let animation_url = metadata.get("animation_url").unwrap().as_str().unwrap();

        assert!(animation_url.contains("ar://test_content_tx_id"));
    }

    #[test]
    fn test_build_metadata_json_external_url_includes_pda() {
        let agreement = create_test_agreement();
        let pda = "my_pda_address";

        let metadata = build_metadata_json(&agreement, pda);
        let external_url = metadata.get("external_url").unwrap().as_str().unwrap();

        assert!(external_url.contains("my_pda_address"));
        assert!(external_url.starts_with("https://pactum.app/agreement/"));
    }

    #[test]
    fn test_build_attributes_contains_status() {
        let agreement = create_test_agreement();
        let attrs = build_attributes(&agreement);

        let status_attr = attrs
            .iter()
            .find(|a| a.get("trait_type").and_then(|t| t.as_str()) == Some("Status"));

        assert!(status_attr.is_some());
        assert!(status_attr.unwrap().get("value").is_some());
    }

    #[test]
    fn test_build_attributes_contains_parties_count() {
        let agreement = create_test_agreement();
        let attrs = build_attributes(&agreement);

        let parties_attr = attrs
            .iter()
            .find(|a| a.get("trait_type").and_then(|t| t.as_str()) == Some("Parties"));

        assert!(parties_attr.is_some());
        let value = parties_attr
            .unwrap()
            .get("value")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(value, "2"); // Test agreement has 2 parties
    }

    #[test]
    fn test_build_attributes_contains_storage_backend() {
        let agreement = create_test_agreement();
        let attrs = build_attributes(&agreement);

        let backend_attr = attrs
            .iter()
            .find(|a| a.get("trait_type").and_then(|t| t.as_str()) == Some("Storage Backend"));

        assert!(backend_attr.is_some());
    }

    #[test]
    fn test_build_attributes_stable_order() {
        let agreement = create_test_agreement();
        let attrs1 = build_attributes(&agreement);
        let attrs2 = build_attributes(&agreement);

        // Verify same order and content on multiple calls (deterministic)
        assert_eq!(attrs1.len(), attrs2.len());
        for (a1, a2) in attrs1.iter().zip(attrs2.iter()) {
            assert_eq!(a1, a2);
        }
    }

    #[test]
    fn test_upload_metadata_json_serializes_to_json() {
        // This test verifies that metadata can be serialized without errors
        let agreement = create_test_agreement();
        let metadata = build_metadata_json(&agreement, "test_pda");

        let json_bytes = serde_json::to_vec(&metadata).expect("should serialize");
        assert!(!json_bytes.is_empty());

        // Verify it round-trips
        let deserialized: Value = serde_json::from_slice(&json_bytes).expect("should deserialize");
        assert_eq!(deserialized, metadata);
    }
}
