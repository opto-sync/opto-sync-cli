use std::fs;

use clap::Parser;
use opto_sync_cli::{execute, Cli, CliError};
use tempfile::TempDir;

fn run(source: &str) -> Result<String, CliError> {
    let dir = TempDir::new().expect("temp root");
    fs::write(dir.path().join("candidate.json"), source).expect("fixture");
    let cli = Cli::try_parse_from([
        "opto-sync", "--root", dir.path().to_str().expect("UTF-8 path"),
        "validate", "--kind", "atto-cursor-model", "--input", "candidate.json",
    ])
    .expect("CLI parsing");
    execute(&cli)
}

#[test]
fn accepts_bounded_positive_and_adversarial_cases_without_authorizing_actions() {
    let source = include_str!("../fixtures/atto-cursor-model.v1.json");
    let response = run(source).expect("frozen hypothetical fixture");
    let value: serde_json::Value = serde_json::from_str(&response).expect("json");
    assert_eq!(value["data"]["kind"], "atto_cursor_model");
    assert_eq!(value["data"]["caseCount"], 5);
    assert_eq!(value["data"]["admittedCount"], 1);
    assert_eq!(value["data"]["rejectedCount"], 4);
    assert_eq!(value["data"]["guestExecuted"], false);
    assert_eq!(value["data"]["authorizationEvidence"], false);
    assert_eq!(value["data"]["redacted"], true);
    assert!(!response.contains("activeGeneration"));
}

#[test]
fn refuses_forged_outcomes_and_vacuous_traces() {
    let base: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/atto-cursor-model.v1.json")).unwrap();
    let mut forged = base.clone();
    forged["cases"][1]["expectedAdmit"] = serde_json::json!(true);
    assert!(matches!(run(&forged.to_string()), Err(CliError::Validation)));
    let mut vacuous = base.clone();
    vacuous["cases"] = serde_json::json!([base["cases"][0]]);
    assert!(matches!(run(&vacuous.to_string()), Err(CliError::Validation)));
    let mut unknown = base.clone();
    unknown["cases"][0]["tenantSecret"] = serde_json::json!("sensitive");
    assert!(matches!(run(&unknown.to_string()), Err(CliError::Validation)));
}

#[test]
fn rejects_noncanonical_negative_fractional_and_overflowing_values() {
    let base: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/atto-cursor-model.v1.json")).unwrap();
    for bad in ["05", "-1", "1.5", "9223372036854775808", "1e2", ""] {
        let mut value = base.clone();
        value["cases"][0]["localCheckpoint"] = serde_json::json!(bad);
        assert!(matches!(run(&value.to_string()), Err(CliError::Validation)));
    }
}
