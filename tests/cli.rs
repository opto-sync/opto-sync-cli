use std::{fs, path::Path};

use clap::Parser;
use opto_sync_cli::{execute, Cli, CliError, Command, ValidationKind, OUTPUT_SCHEMA};
use tempfile::TempDir;

fn request(root: &Path, command: Command) -> Cli {
    Cli {
        root: root.to_path_buf(),
        max_input_bytes: 1_048_576,
        max_output_bytes: 32_768,
        command,
    }
}

#[test]
fn replays_the_committed_policy_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cli = request(
        root,
        Command::Replay {
            input: "fixtures/traces.v1.json".into(),
        },
    );
    let output = execute(&cli).expect("fixture should replay");
    let value: serde_json::Value = serde_json::from_str(&output).expect("versioned JSON output");
    assert_eq!(value["schema"], OUTPUT_SCHEMA);
    assert_eq!(value["data"]["traceCount"], 3);
    assert_eq!(value["data"]["eventCount"], 14);
}

#[test]
fn rejects_paths_outside_the_canonical_root() {
    let root = TempDir::new().expect("root");
    let outside = TempDir::new().expect("outside");
    let input = outside.path().join("outside.json");
    fs::write(&input, b"{}").expect("fixture");
    let cli = request(
        root.path(),
        Command::Validate {
            kind: ValidationKind::Json,
            input,
        },
    );
    assert!(matches!(execute(&cli), Err(CliError::Input)));
}

#[test]
fn rejects_malformed_and_oversized_inputs() {
    let root = TempDir::new().expect("root");
    fs::write(root.path().join("malformed.json"), b"{not-json").expect("fixture");
    fs::write(root.path().join("large.json"), vec![b' '; 65]).expect("fixture");

    let malformed = request(
        root.path(),
        Command::Validate {
            kind: ValidationKind::Json,
            input: "malformed.json".into(),
        },
    );
    assert!(matches!(execute(&malformed), Err(CliError::Validation)));

    let mut oversized = request(
        root.path(),
        Command::Validate {
            kind: ValidationKind::Json,
            input: "large.json".into(),
        },
    );
    oversized.max_input_bytes = 64;
    assert!(matches!(execute(&oversized), Err(CliError::Input)));
}

#[test]
fn treats_command_shaped_paths_as_plain_files() {
    let root = TempDir::new().expect("root");
    let input = "$(touch-owned).json";
    fs::write(root.path().join(input), b"{}").expect("fixture");
    let cli = request(
        root.path(),
        Command::Validate {
            kind: ValidationKind::Json,
            input: input.into(),
        },
    );
    assert!(execute(&cli).is_ok());
    assert!(!root.path().join("owned").exists());
}

#[test]
fn inspect_never_echoes_secret_or_tenant_content() {
    let root = TempDir::new().expect("root");
    fs::write(
        root.path().join("diagnostic.json"),
        br#"{"authorization":"Bearer TOP_SECRET","tenantId":"TENANT-42","payload":{"email":"private@example.test"}}"#,
    )
    .expect("fixture");
    let cli = request(
        root.path(),
        Command::Inspect {
            input: "diagnostic.json".into(),
        },
    );
    let output = execute(&cli).expect("redacted inspection");
    assert!(!output.contains("TOP_SECRET"));
    assert!(!output.contains("TENANT-42"));
    assert!(!output.contains("private@example.test"));
    assert!(output.contains("sensitiveKeyCount"));
}

#[test]
fn rejects_trace_divergence_and_duplicate_engine_ownership() {
    let root = TempDir::new().expect("root");
    fs::write(
        root.path().join("bad-trace.json"),
        br#"{"schema":"opto-sync.optimism-traces.v1","traces":[{"name":"bad","strategy":"remote_confirmed","events":["local_accepted"],"states":["proposed","confirmed"]}]}"#,
    )
    .expect("fixture");
    fs::write(
        root.path().join("Cargo.lock"),
        br#"version = 4

[[package]]
name = "syncer-rs"
version = "1.0.0"

[[package]]
name = "syncer-c"
version = "1.0.0"
"#,
    )
    .expect("fixture");

    let replay = request(
        root.path(),
        Command::Replay {
            input: "bad-trace.json".into(),
        },
    );
    assert!(matches!(execute(&replay), Err(CliError::Replay)));

    let lock = request(
        root.path(),
        Command::Validate {
            kind: ValidationKind::CargoLock,
            input: "Cargo.lock".into(),
        },
    );
    assert!(matches!(execute(&lock), Err(CliError::Validation)));
}

#[test]
fn enforces_output_bounds_and_stable_usage_code() {
    let root = TempDir::new().expect("root");
    fs::write(root.path().join("input.json"), b"{}").expect("fixture");
    let mut cli = request(
        root.path(),
        Command::Inspect {
            input: "input.json".into(),
        },
    );
    cli.max_output_bytes = 64;
    assert!(matches!(execute(&cli), Err(CliError::OutputBound)));
    assert_eq!(CliError::Input.exit_code(), 3);
    assert_eq!(CliError::Validation.exit_code(), 4);
    assert_eq!(CliError::Replay.exit_code(), 5);

    let usage = Cli::try_parse_from(["opto-sync"]).expect_err("subcommand is required");
    assert_eq!(usage.exit_code(), 2);
}
