#![forbid(unsafe_code)]

use std::io;
use std::process::ExitCode;

use clap::Parser;
use opto_sync_cli::{execute, render_failure, Cli};
use ores_clis_core::{
    paint, parse_shared_argv, top_level_io, CliPolicy, ColorRole, EmitDisposition,
    EnvironmentHints, LogLevel, OutputMode, ProtocolEmitter, RuntimePolicy, StreamRole,
    TerminalState,
};

fn emit_error(runtime: RuntimePolicy, message: impl std::fmt::Display) {
    if !runtime.allows_log(LogLevel::Error) {
        return;
    }
    let stderr = io::stderr();
    let mut emitter = ProtocolEmitter::new(stderr.lock(), StreamRole::Diagnostics);
    let line = paint(runtime.color_stderr(), ColorRole::Error, message);
    let _ = top_level_io(emitter.emit_diagnostic_line(&line));
}

fn main() -> ExitCode {
    let terminals = TerminalState::detect();
    let environment = EnvironmentHints::detect();
    let mut process_args = std::env::args();
    let program = process_args
        .next()
        .unwrap_or_else(|| "opto-sync".to_owned());
    let shared = match parse_shared_argv(process_args) {
        Ok(shared) => shared,
        Err(error) => {
            let runtime = CliPolicy::default().resolve(terminals, environment);
            emit_error(runtime, format!("opto-sync: {error}"));
            return ExitCode::from(2);
        }
    };

    let runtime = CliPolicy {
        output: OutputMode::Json,
        color: shared.policy.color,
        log_level: shared.policy.log_level,
    }
    .resolve(terminals, environment);

    if shared.output_was_explicit() && matches!(shared.policy.output, OutputMode::Human) {
        emit_error(
            runtime,
            "opto-sync: human output is unsupported; stdout is the versioned JSON protocol",
        );
        return ExitCode::from(2);
    }

    let mut argv = Vec::with_capacity(shared.passthrough.len() + 1);
    argv.push(program);
    argv.extend(shared.passthrough);
    let cli = Cli::parse_from(argv);

    match execute(&cli) {
        Ok(output) => {
            let stdout = io::stdout();
            let mut emitter = ProtocolEmitter::new(stdout.lock(), StreamRole::Primary);
            match top_level_io(emitter.emit_primary_machine_record(&output)) {
                Ok(EmitDisposition::Written | EmitDisposition::ConsumerClosed) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(70),
            }
        }
        Err(error) => {
            if runtime.allows_log(LogLevel::Error) {
                let stderr = io::stderr();
                let mut emitter = ProtocolEmitter::new(stderr.lock(), StreamRole::Diagnostics);
                let _ =
                    top_level_io(emitter.emit_diagnostic_line(&render_failure("request", &error)));
            }
            ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(70))
        }
    }
}
