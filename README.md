# opto-sync-cli

Operator and developer CLI for Opto Sync validation, simulation, packaging,
and diagnostics.

## Status

Bootstrap repository. There is no published binary or package yet. The CLI will
not become a deployment control plane and will not mutate production data by
default.

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

## First implementation gates

- Define a versioned JSON output and stable exit-code contract before command
  implementation.
- Separate read-only validation from mutating operations at the command and
  capability layers.
- Add deterministic replay and dependency-graph diagnostics before network or
  database mutation commands.
- Bound input sizes, timeouts, retries, output volume, and subprocess behavior.
- Redact credentials, authorization headers, tenant data, and local payloads by
  default; prove redaction with adversarial fixtures.
- Keep package publication disabled until clean-room installation and rollback
  evidence exists for every supported host platform.
