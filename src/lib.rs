#![forbid(unsafe_code)]

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand, ValueEnum};
use opto_sync_lib::{decide, OptimismStrategy, WriteEvent, WriteState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

pub const OUTPUT_SCHEMA: &str = "opto-sync.cli.output.v1";
pub const INTERFACES_REVISION: &str = "b92b3a2eb43eeb183144521a188ae465a013951e";
pub const POLICY_REVISION: &str = "f2ea017328aff58401d38a6d36480c45b39d3c15";

const MAX_JSON_NODES: usize = 100_000;
const MAX_TRACE_COUNT: usize = 128;
const MAX_TRACE_EVENTS: usize = 1_024;

#[derive(Debug, Parser)]
#[command(name = "opto-sync", version, about)]
pub struct Cli {
    /// Canonical trust root for every input path.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Maximum bytes read from any one input file.
    #[arg(
        long,
        default_value_t = 1_048_576,
        value_parser = clap::value_parser!(u64).range(64..=8_388_608)
    )]
    pub max_input_bytes: u64,

    /// Maximum bytes emitted in the versioned JSON response.
    #[arg(
        long,
        default_value_t = 32_768,
        value_parser = clap::value_parser!(u64).range(512..=65_536)
    )]
    pub max_output_bytes: u64,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate a bounded local artifact without changing it.
    Validate {
        #[arg(long, value_enum)]
        kind: ValidationKind,
        #[arg(long)]
        input: PathBuf,
    },
    /// Deterministically replay bounded policy traces.
    Replay {
        #[arg(long)]
        input: PathBuf,
    },
    /// Report redacted JSON shape metrics without echoing keys or values.
    Inspect {
        #[arg(long)]
        input: PathBuf,
    },
}

impl Command {
    const fn name(&self) -> &'static str {
        match self {
            Self::Validate { .. } => "validate",
            Self::Replay { .. } => "replay",
            Self::Inspect { .. } => "inspect",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ValidationKind {
    Json,
    Traces,
    CargoLock,
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error("input rejected by bounded path policy")]
    Input,
    #[error("artifact failed validation")]
    Validation,
    #[error("deterministic replay failed")]
    Replay,
    #[error("response exceeded the configured output bound")]
    OutputBound,
    #[error("internal serialization failed")]
    Internal,
}

impl CliError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Input => "input_policy",
            Self::Validation => "validation",
            Self::Replay => "replay",
            Self::OutputBound => "output_bound",
            Self::Internal => "internal",
        }
    }

    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Input => 3,
            Self::Validation => 4,
            Self::Replay => 5,
            Self::OutputBound => 6,
            Self::Internal => 70,
        }
    }
}

#[derive(Debug)]
struct InputPolicy {
    root: PathBuf,
    maximum_bytes: u64,
}

impl InputPolicy {
    fn new(root: &Path, maximum_bytes: u64) -> Result<Self, CliError> {
        let root = fs::canonicalize(root).map_err(|_| CliError::Input)?;
        if !root.is_dir() {
            return Err(CliError::Input);
        }
        Ok(Self {
            root,
            maximum_bytes,
        })
    }

    fn read(&self, input: &Path) -> Result<Vec<u8>, CliError> {
        let candidate = if input.is_absolute() {
            input.to_path_buf()
        } else {
            self.root.join(input)
        };
        let canonical = fs::canonicalize(candidate).map_err(|_| CliError::Input)?;
        if !canonical.starts_with(&self.root) {
            return Err(CliError::Input);
        }
        let metadata = fs::metadata(&canonical).map_err(|_| CliError::Input)?;
        if !metadata.is_file() || metadata.len() > self.maximum_bytes {
            return Err(CliError::Input);
        }
        let bytes = fs::read(canonical).map_err(|_| CliError::Input)?;
        if bytes.len() as u64 > self.maximum_bytes {
            return Err(CliError::Input);
        }
        Ok(bytes)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    schema: &'static str,
    ok: bool,
    command: &'static str,
    interfaces_revision: &'static str,
    policy_revision: &'static str,
    data: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FailureEnvelope<'a> {
    schema: &'static str,
    ok: bool,
    command: &'a str,
    error: FailureDetail<'a>,
}

#[derive(Debug, Serialize)]
struct FailureDetail<'a> {
    code: &'a str,
    message: &'a str,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonShape {
    nodes: usize,
    maximum_depth: usize,
    objects: usize,
    arrays: usize,
    scalar_values: usize,
    sensitive_key_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TraceSet {
    schema: String,
    traces: Vec<Trace>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trace {
    name: String,
    strategy: OptimismStrategy,
    events: Vec<WriteEvent>,
    states: Vec<WriteState>,
}

#[derive(Debug, Deserialize)]
struct CargoLock {
    version: u32,
    #[serde(default)]
    package: Vec<LockPackage>,
}

#[derive(Debug, Deserialize)]
struct LockPackage {
    name: String,
    source: Option<String>,
}

pub fn execute(cli: &Cli) -> Result<String, CliError> {
    let policy = InputPolicy::new(&cli.root, cli.max_input_bytes)?;
    let data = match &cli.command {
        Command::Validate { kind, input } => match kind {
            ValidationKind::Json => {
                let value = parse_json(&policy.read(input)?)?;
                let shape = inspect_json(&value)?;
                json!({ "kind": "json", "shape": shape })
            }
            ValidationKind::Traces => replay_bytes(&policy.read(input)?)?,
            ValidationKind::CargoLock => validate_cargo_lock(&policy.read(input)?)?,
        },
        Command::Replay { input } => replay_bytes(&policy.read(input)?)?,
        Command::Inspect { input } => {
            let value = parse_json(&policy.read(input)?)?;
            let shape = inspect_json(&value)?;
            json!({ "redacted": true, "shape": shape })
        }
    };

    let envelope = Envelope {
        schema: OUTPUT_SCHEMA,
        ok: true,
        command: cli.command.name(),
        interfaces_revision: INTERFACES_REVISION,
        policy_revision: POLICY_REVISION,
        data,
    };
    let rendered = serde_json::to_string(&envelope).map_err(|_| CliError::Internal)?;
    if rendered.len() as u64 > cli.max_output_bytes {
        return Err(CliError::OutputBound);
    }
    Ok(rendered)
}

pub fn render_failure(command: &str, error: &CliError) -> String {
    let message = error.to_string();
    let envelope = FailureEnvelope {
        schema: OUTPUT_SCHEMA,
        ok: false,
        command,
        error: FailureDetail {
            code: error.code(),
            message: &message,
        },
    };
    serde_json::to_string(&envelope).unwrap_or_else(|_| {
        String::from(
            r#"{"schema":"opto-sync.cli.output.v1","ok":false,"command":"unknown","error":{"code":"internal","message":"internal serialization failed"}}"#,
        )
    })
}

fn parse_json(bytes: &[u8]) -> Result<Value, CliError> {
    serde_json::from_slice(bytes).map_err(|_| CliError::Validation)
}

fn inspect_json(value: &Value) -> Result<JsonShape, CliError> {
    let mut shape = JsonShape::default();
    let mut pending = vec![(value, 1_usize)];
    while let Some((current, depth)) = pending.pop() {
        shape.nodes = shape.nodes.checked_add(1).ok_or(CliError::Validation)?;
        if shape.nodes > MAX_JSON_NODES {
            return Err(CliError::Validation);
        }
        shape.maximum_depth = shape.maximum_depth.max(depth);
        match current {
            Value::Object(object) => {
                shape.objects += 1;
                for (key, child) in object {
                    if is_sensitive_key(key) {
                        shape.sensitive_key_count += 1;
                    }
                    pending.push((child, depth + 1));
                }
            }
            Value::Array(array) => {
                shape.arrays += 1;
                pending.extend(array.iter().map(|child| (child, depth + 1)));
            }
            _ => shape.scalar_values += 1,
        }
    }
    Ok(shape)
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "email",
        "owner",
        "payload",
        "secret",
        "tenant",
        "token",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn replay_bytes(bytes: &[u8]) -> Result<Value, CliError> {
    let trace_set: TraceSet = serde_json::from_slice(bytes).map_err(|_| CliError::Validation)?;
    if trace_set.schema != "opto-sync.optimism-traces.v1"
        || trace_set.traces.is_empty()
        || trace_set.traces.len() > MAX_TRACE_COUNT
    {
        return Err(CliError::Validation);
    }

    let mut names = HashSet::with_capacity(trace_set.traces.len());
    let mut event_count = 0_usize;
    for trace in &trace_set.traces {
        if trace.name.is_empty()
            || trace.name.len() > 128
            || !names.insert(trace.name.as_str())
            || trace.events.is_empty()
            || trace.events.len() > MAX_TRACE_EVENTS
            || trace.states.len() != trace.events.len() + 1
            || trace.states.first() != Some(&WriteState::Proposed)
        {
            return Err(CliError::Validation);
        }

        let mut current = WriteState::Proposed;
        for (index, event) in trace.events.iter().copied().enumerate() {
            let transition =
                decide(trace.strategy, current, event).map_err(|_| CliError::Replay)?;
            if trace.states.get(index + 1) != Some(&transition.next) {
                return Err(CliError::Replay);
            }
            current = transition.next;
        }
        event_count = event_count
            .checked_add(trace.events.len())
            .ok_or(CliError::Validation)?;
    }

    Ok(json!({
        "kind": "traces",
        "schema": trace_set.schema,
        "traceCount": trace_set.traces.len(),
        "eventCount": event_count,
        "deterministic": true
    }))
}

fn validate_cargo_lock(bytes: &[u8]) -> Result<Value, CliError> {
    let text = std::str::from_utf8(bytes).map_err(|_| CliError::Validation)?;
    let lock: CargoLock = toml::from_str(text).map_err(|_| CliError::Validation)?;
    if !(3..=4).contains(&lock.version) || lock.package.len() > 100_000 {
        return Err(CliError::Validation);
    }

    let mut engine_packages = 0_usize;
    let mut pinned_git_sources = 0_usize;
    let mut unpinned_git_sources = 0_usize;
    for package in &lock.package {
        if matches!(package.name.as_str(), "syncer" | "syncer-c" | "syncer-rs") {
            engine_packages += 1;
        }
        if let Some(source) = package
            .source
            .as_deref()
            .filter(|value| value.starts_with("git+"))
        {
            let pinned = source
                .rsplit_once('#')
                .map(|(_, revision)| {
                    revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .unwrap_or(false);
            if pinned {
                pinned_git_sources += 1;
            } else {
                unpinned_git_sources += 1;
            }
        }
    }

    if engine_packages > 1 || unpinned_git_sources > 0 {
        return Err(CliError::Validation);
    }
    Ok(json!({
        "kind": "cargo_lock",
        "formatVersion": lock.version,
        "packageCount": lock.package.len(),
        "enginePackageCount": engine_packages,
        "engineUnique": true,
        "pinnedGitSourceCount": pinned_git_sources,
        "unpinnedGitSourceCount": unpinned_git_sources
    }))
}
