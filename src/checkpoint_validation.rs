//! Read-only, fail-closed validation for batches of monotonic checkpoints.
use opto_sync_lib::{CheckpointDecision, CheckpointLedger, MAX_BATCH_OBSERVATIONS};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::CliError;

const MAX_OPERATIONS: usize = MAX_BATCH_OBSERVATIONS;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Batch {
    schema: String,
    initial_checkpoint: String,
    maximum_seen_operations: usize,
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Operation {
    operation_id: String,
    checkpoint: String,
}

/// Validate every operation before returning any result. No operation IDs or
/// source payloads are included in the response.
pub(crate) fn validate(bytes: &[u8]) -> Result<Value, CliError> {
    let batch: Batch = serde_json::from_slice(bytes).map_err(|_| CliError::Validation)?;
    if batch.schema != "opto-sync.checkpoint-batch.v1"
        || batch.operations.is_empty()
        || batch.operations.len() > MAX_OPERATIONS
        || batch.maximum_seen_operations == 0
        || batch.maximum_seen_operations > MAX_OPERATIONS
    {
        return Err(CliError::Validation);
    }

    // The pinned policy exposes pure checkpoint transitions. Build a new
    // temporary ledger for every admitted observation; publish only once
    // the entire batch succeeds and never persist any intermediate snapshot.
    let ledger = CheckpointLedger::new(&batch.initial_checkpoint, batch.maximum_seen_operations)
        .map_err(|_| CliError::Validation)?;

    let (ledger, advanced, duplicates) = batch.operations.iter().try_fold(
        (ledger, 0_usize, 0_usize),
        |(ledger, advanced, duplicates), operation| {
            let (next, decision) = ledger
                .observe(&operation.operation_id, &operation.checkpoint)
                .map_err(|_| CliError::Validation)?;
            let (advanced, duplicates) = match decision {
                CheckpointDecision::Advanced => (advanced + 1, duplicates),
                CheckpointDecision::Duplicate => (advanced, duplicates + 1),
            };
            Ok::<_, CliError>((next, advanced, duplicates))
        },
    )?;

    Ok(json!({
        "kind": "checkpoint_batch",
        "schema": batch.schema,
        "operationCount": batch.operations.len(),
        "advancedCount": advanced,
        "duplicateCount": duplicates,
        "finalCheckpoint": ledger.checkpoint(),
        "redacted": true
    }))
}
