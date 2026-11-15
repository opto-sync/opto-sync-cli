//! Read-only, hypothetical AttoLang scoped cursor policy model.
//! Input assertions are untrusted: a passing model is NEVER authorization or
//! evidence that a guest compiler, durable host adapter, or server has run.
use serde::Deserialize;
use serde_json::{json, Value};

use crate::CliError;

const MAX_CASES: usize = 1_024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fixture {
    schema: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    active_generation: String,
    observed_generation: String,
    local_checkpoint: String,
    previous_checkpoint: String,
    next_checkpoint: String,
    scope_verified: bool,
    authoritative: bool,
    expected_admit: bool,
}

fn canonical_i64(input: &str) -> Result<i64, CliError> {
    if input.is_empty()
        || (input.len() > 1 && input.starts_with('0'))
        || !input.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(CliError::Validation);
    }
    input.parse::<i64>().map_err(|_| CliError::Validation)
}

/// Evaluate bounded pure predicates only. Never publish a trusted receipt.
pub(crate) fn validate(bytes: &[u8]) -> Result<Value, CliError> {
    let fixture: Fixture = serde_json::from_slice(bytes).map_err(|_| CliError::Validation)?;
    if fixture.schema != "opto-sync.atto-cursor-model.v1"
        || fixture.cases.is_empty()
        || fixture.cases.len() > MAX_CASES
    {
        return Err(CliError::Validation);
    }
    let mut admitted = 0_usize;
    let mut rejected = 0_usize;
    let mut stale = false;
    let mut wrong_scope = false;
    let mut replay = false;
    for case in &fixture.cases {
        let active = canonical_i64(&case.active_generation)?;
        let observed = canonical_i64(&case.observed_generation)?;
        let local = canonical_i64(&case.local_checkpoint)?;
        let previous = canonical_i64(&case.previous_checkpoint)?;
        let next = canonical_i64(&case.next_checkpoint)?;
        let accepted = active > 0
            && active == observed
            && case.scope_verified
            && case.authoritative
            && previous == local
            && next > previous;
        if accepted != case.expected_admit {
            return Err(CliError::Validation);
        }
        if accepted {
            admitted += 1;
        } else {
            rejected += 1;
        }
        stale |= active > 0 && active != observed && !accepted;
        wrong_scope |= !case.scope_verified && !accepted;
        replay |= next <= local && !accepted;
    }
    if admitted == 0 || rejected == 0 || !stale || !wrong_scope || !replay {
        return Err(CliError::Validation);
    }
    Ok(json!({
        "kind": "atto_cursor_model",
        "schema": fixture.schema,
        "caseCount": fixture.cases.len(),
        "admittedCount": admitted,
        "rejectedCount": rejected,
        "guestExecuted": false,
        "authorizationEvidence": false,
        "redacted": true
    }))
}
