use std::fs;

use clap::Parser;
use opto_sync_cli::{execute, Cli, CliError};
use tempfile::TempDir;

fn run(source: &str) -> Result<String, CliError> {
    let dir = TempDir::new().expect("temp root");
    fs::write(dir.path().join("batch.json"), source).expect("write input");
    let cli = Cli::try_parse_from([
        "opto-sync",
        "--root",
        dir.path().to_str().expect("utf8 tempdir"),
        "validate",
        "--kind",
        "checkpoint-batch",
        "--input",
        "batch.json",
    ])
    .expect("parse CLI flags");
    execute(&cli)
}

#[test]
fn validates_ordered_batch_with_redacted_output() {
    let output = run(r#"{"schema":"opto-sync.checkpoint-batch.v1","initialCheckpoint":"0","maximumSeenOperations":3,"operations":[{"operationId":"sensitive-operation-a","checkpoint":"1"},{"operationId":"sensitive-operation-a","checkpoint":"1"},{"operationId":"sensitive-operation-b","checkpoint":"2"}]}"#).unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["kind"], "checkpoint_batch");
    assert_eq!(json["data"]["advancedCount"], 2);
    assert_eq!(json["data"]["duplicateCount"], 1);
    assert_eq!(json["data"]["finalCheckpoint"], "2");
    assert!(!output.contains("sensitive-operation"));
}

#[test]
fn rejects_regression_or_noncanonical_checkpoint_without_partial_success() {
    for source in [
        r#"{"schema":"opto-sync.checkpoint-batch.v1","initialCheckpoint":"0","maximumSeenOperations":3,"operations":[{"operationId":"a","checkpoint":"1"},{"operationId":"b","checkpoint":"0"}]}"#,
        r#"{"schema":"opto-sync.checkpoint-batch.v1","initialCheckpoint":"0","maximumSeenOperations":3,"operations":[{"operationId":"a","checkpoint":"1"},{"operationId":"b","checkpoint":"01"}]}"#,
        r#"{"schema":"opto-sync.checkpoint-batch.v1","initialCheckpoint":"0","maximumSeenOperations":3,"operations":[{"operationId":"a","checkpoint":"1"},{"operationId":"bad id","checkpoint":"2"}]}"#,
    ] {
        assert!(matches!(run(source), Err(CliError::Validation)));
    }
}

#[test]
fn rejects_window_exhaustion_unknown_fields_and_empty_batches() {
    for source in [
        r#"{"schema":"opto-sync.checkpoint-batch.v1","initialCheckpoint":"0","maximumSeenOperations":1,"operations":[{"operationId":"a","checkpoint":"1"},{"operationId":"b","checkpoint":"2"}]}"#,
        r#"{"schema":"opto-sync.checkpoint-batch.v1","initialCheckpoint":"0","maximumSeenOperations":1,"operations":[]}"#,
        r#"{"schema":"opto-sync.checkpoint-batch.v1","initialCheckpoint":"0","maximumSeenOperations":3,"operations":[{"operationId":"a","checkpoint":"1","tenantId":"secret"}]}"#,
    ] {
        assert!(matches!(run(source), Err(CliError::Validation)));
    }
}
