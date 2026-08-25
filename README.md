# opto-sync-cli

Operator and developer CLI for Opto Sync validation, simulation, packaging,
and diagnostics.

## Status

Implemented read-only CLI; package publication and every production mutation
remain disabled. The binary pins `opto-sync-interfaces` at
`b92b3a2eb43eeb183144521a188ae465a013951e` and `opto-sync-lib` at
`f2ea017328aff58401d38a6d36480c45b39d3c15`. It installs no merge engine and
has no network, database, subprocess, or filesystem-write capability.

## Commands and output contract

```sh
cargo run --locked -- validate --kind json --input manifest.json
cargo run --locked -- validate --kind cargo-lock --input Cargo.lock
cargo run --locked -- replay --input fixtures/traces.v1.json
cargo run --locked -- inspect --input local-diagnostic.json
```

Every successful response uses `opto-sync.cli.output.v1`, records the exact
interfaces and policy revisions, and is bounded by `--max-output-bytes`.
`inspect` returns counts and structural metrics only: it never emits input
keys or values. Every input is canonicalized beneath `--root`, must be a regular
file, and is bounded by `--max-input-bytes` before parsing.

Stable process exit codes are `0` for success, `2` for command-line usage, `3`
for input-policy rejection, `4` for invalid artifacts, `5` for trace divergence,
`6` for an output-bound violation, and `70` for internal serialization failure.

## Ownership boundary

The CLI may provide explicit, auditable commands to:

- validate interface schemas, fixtures, manifests, and immutable locks;
- replay deterministic histories and formal-method traces against selected
  client or engine implementations;
- inspect local queue, checkpoint, retry, and connectivity state with secrets
  and user content redacted by default;
- verify packaging, dependency-boundary, and clean-room consumer contracts;
- exercise opt-in development diagnostics for IndexedDB, SQLite, PostgreSQL,
  Supabase, HTTP, WebSocket, and TCP adapters.

The CLI must not silently apply production DDL, bypass tenant authorization,
publish packages, dispatch background work, or rewrite checkpoints. Mutating
commands require an explicit target, confirmation or non-interactive approval
flag, bounded scope, and a machine-readable receipt.

## Required dependency graph

The intended graph is `interfaces -> lib -> clients -> cli`. The CLI may select
one engine implementation through the client capability boundary, but it must
not install another copy of `syncer.c` or `syncer.rs`.

This first read-only surface directly consumes the interface and policy crates
because the clients repository does not yet expose its multi-runtime package
surface as a single CLI dependency. Lock inspection enforces at most one engine
in a final composition; adding a client adapter is deferred until it consumes
the same immutable interfaces and policy revisions.

## Implemented safety gates

- `validate` parses bounded JSON, policy traces, or Cargo locks and rejects
  unpinned Git sources and duplicate merge-engine ownership.
- `replay` executes committed histories through the pinned policy state machine
  and rejects unexpected states or invalid transitions.
- `inspect` reports only redacted shape metrics and sensitive-key counts.
- Adversarial tests cover path traversal, malformed and oversized input,
  command-shaped paths, secret-shaped data, trace divergence, output bounds,
  and duplicate engines.
- CI rejects mutation/network authority in `src`, runs strict Rust gates, and
  proves publication remains disabled in `.zpkg.toml`.

## Validation

```sh
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo run --locked -- replay --input fixtures/traces.v1.json
```
