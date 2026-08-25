#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;
use opto_sync_cli::{execute, render_failure, Cli};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(&cli) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", render_failure("request", &error));
            ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(70))
        }
    }
}
