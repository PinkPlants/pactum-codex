//! Anchor Program Log Parsing
//!
//! Parses Solana program logs to extract Pactum instruction events.

use serde::Deserialize;

/// A parsed program log entry from Solana
#[derive(Debug, Clone)]
pub struct ProgramLog {
    pub signature: String,
    pub slot: u64,
    pub logs: Vec<String>,
}

/// Types of Pactum instructions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstructionType {
    CreateAgreement,
    SignAgreement,
    CancelAgreement,
    ExpireAgreement,
    VoteRevoke,
}

impl InstructionType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "CreateAgreement" => Some(Self::CreateAgreement),
            "SignAgreement" => Some(Self::SignAgreement),
            "CancelAgreement" => Some(Self::CancelAgreement),
            "ExpireAgreement" => Some(Self::ExpireAgreement),
            "VoteRevoke" => Some(Self::VoteRevoke),
            _ => None,
        }
    }
}

/// Parsed event data from program logs
#[derive(Debug, Clone)]
pub struct ParsedEvent {
    pub instruction: InstructionType,
    pub agreement_pda: String,
    pub creator: Option<String>,
    pub signer: Option<String>,
    pub parties: Vec<String>,
}

/// Raw log data structure for JSON parsing
#[derive(Debug, Deserialize)]
struct LogData {
    #[serde(rename = "agreement_pda")]
    agreement_pda: Option<String>,
    #[serde(rename = "creator")]
    creator: Option<String>,
    #[serde(rename = "signer")]
    signer: Option<String>,
    #[serde(rename = "parties")]
    parties: Option<Vec<String>>,
}

/// Parse program logs to extract instruction events
///
/// Returns Some(ParsedEvent) if a Pactum instruction is found and parsed,
/// Returns None if logs don't contain a recognized instruction.
pub fn parse_logs(signature: &str, slot: u64, logs: &[String]) -> Option<ParsedEvent> {
    let mut instruction: Option<InstructionType> = None;
    let mut agreement_pda: Option<String> = None;
    let mut creator: Option<String> = None;
    let mut signer: Option<String> = None;
    let mut parties: Vec<String> = Vec::new();

    for log in logs {
        // Look for instruction name
        if let Some(instr_str) = log.strip_prefix("Program log: Instruction: ") {
            instruction = InstructionType::from_str(instr_str.trim());
            continue;
        }

        // Try to parse JSON data from log lines
        if log.starts_with("Program log: {") {
            if let Ok(data) =
                serde_json::from_str::<LogData>(log.trim_start_matches("Program log: "))
            {
                if let Some(pda) = data.agreement_pda {
                    agreement_pda = Some(pda);
                }
                if let Some(c) = data.creator {
                    creator = Some(c);
                }
                if let Some(s) = data.signer {
                    signer = Some(s);
                }
                if let Some(p) = data.parties {
                    parties = p;
                }
            }
        }

        // Alternative: parse simple key=value format
        if let Some(kv) = log.strip_prefix("Program log: ") {
            if kv.starts_with("agreement_pda=") {
                agreement_pda = Some(kv.trim_start_matches("agreement_pda=").to_string());
            } else if kv.starts_with("creator=") {
                creator = Some(kv.trim_start_matches("creator=").to_string());
            } else if kv.starts_with("signer=") {
                signer = Some(kv.trim_start_matches("signer=").to_string());
            }
        }
    }

    // Build result if we found an instruction
    instruction.map(|instr| ParsedEvent {
        instruction: instr,
        agreement_pda: agreement_pda.unwrap_or_else(|| signature.to_string()),
        creator,
        signer,
        parties,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_agreement() {
        let logs = vec![
            "Program DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P invoke [1]".to_string(),
            "Program log: Instruction: CreateAgreement".to_string(),
            "Program log: {\"agreement_pda\": \"ABC123\", \"creator\": \"CreatorPubkey1111111111111111111111111111111\", \"parties\": [\"Party1Pubkey1111111111111111111111111111111\", \"Party2Pubkey1111111111111111111111111111111\"]}".to_string(),
            "Program DF1cHTN9EE8Qonda1esTeYvFjmbYcoc52vDTjTMKvS1P success".to_string(),
        ];

        let result = parse_logs("txsig123", 100, &logs);
        assert!(result.is_some());

        let event = result.unwrap();
        assert_eq!(event.instruction, InstructionType::CreateAgreement);
        assert_eq!(event.agreement_pda, "ABC123");
        assert_eq!(
            event.creator,
            Some("CreatorPubkey1111111111111111111111111111111".to_string())
        );
        assert_eq!(event.parties.len(), 2);
    }

    #[test]
    fn test_parse_sign_agreement() {
        let logs = vec![
            "Program log: Instruction: SignAgreement".to_string(),
            "Program log: {\"agreement_pda\": \"ABC123\", \"signer\": \"SignerPubkey11111111111111111111111111111111\"}".to_string(),
        ];

        let result = parse_logs("txsig456", 200, &logs);
        assert!(result.is_some());

        let event = result.unwrap();
        assert_eq!(event.instruction, InstructionType::SignAgreement);
        assert_eq!(event.agreement_pda, "ABC123");
        assert_eq!(
            event.signer,
            Some("SignerPubkey11111111111111111111111111111111".to_string())
        );
    }

    #[test]
    fn test_parse_cancel_agreement() {
        let logs = vec![
            "Program log: Instruction: CancelAgreement".to_string(),
            "Program log: agreement_pda=XYZ789".to_string(),
        ];

        let result = parse_logs("txsig789", 300, &logs);
        assert!(result.is_some());

        let event = result.unwrap();
        assert_eq!(event.instruction, InstructionType::CancelAgreement);
        assert_eq!(event.agreement_pda, "XYZ789");
    }

    #[test]
    fn test_parse_expire_agreement() {
        let logs = vec![
            "Program log: Instruction: ExpireAgreement".to_string(),
            "Program log: agreement_pda=EXPIRED1".to_string(),
        ];

        let result = parse_logs("txsig000", 400, &logs);
        assert!(result.is_some());

        let event = result.unwrap();
        assert_eq!(event.instruction, InstructionType::ExpireAgreement);
        assert_eq!(event.agreement_pda, "EXPIRED1");
    }

    #[test]
    fn test_parse_vote_revoke() {
        let logs = vec![
            "Program log: Instruction: VoteRevoke".to_string(),
            "Program log: {\"agreement_pda\": \"REVOKE1\", \"signer\": \"VoterPubkey11111111111111111111111111111111\"}".to_string(),
        ];

        let result = parse_logs("txsig111", 500, &logs);
        assert!(result.is_some());

        let event = result.unwrap();
        assert_eq!(event.instruction, InstructionType::VoteRevoke);
        assert_eq!(event.agreement_pda, "REVOKE1");
    }

    #[test]
    fn test_parse_unknown_instruction() {
        let logs = vec!["Program log: Instruction: UnknownInstruction".to_string()];

        let result = parse_logs("txsig222", 600, &logs);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_no_instruction() {
        let logs = vec!["Program log: Some random log".to_string()];

        let result = parse_logs("txsig333", 700, &logs);
        assert!(result.is_none());
    }

    #[test]
    fn test_instruction_type_from_str() {
        assert_eq!(
            InstructionType::from_str("CreateAgreement"),
            Some(InstructionType::CreateAgreement)
        );
        assert_eq!(
            InstructionType::from_str("SignAgreement"),
            Some(InstructionType::SignAgreement)
        );
        assert_eq!(
            InstructionType::from_str("CancelAgreement"),
            Some(InstructionType::CancelAgreement)
        );
        assert_eq!(
            InstructionType::from_str("ExpireAgreement"),
            Some(InstructionType::ExpireAgreement)
        );
        assert_eq!(
            InstructionType::from_str("VoteRevoke"),
            Some(InstructionType::VoteRevoke)
        );
        assert_eq!(InstructionType::from_str("Unknown"), None);
    }
}
