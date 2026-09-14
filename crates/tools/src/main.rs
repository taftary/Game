//! Developer tooling entry point: asset validation and preprocessing.
//!
//! Target: deterministic tools operating on defined engine formats. Current:
//! command dispatcher stub; no tool commands exist yet.

use std::env;
use std::process::ExitCode;

const USAGE: &str = "usage: planet-crafter-tools <command>

Planned commands:
  validate    validate asset inputs
  preprocess  preprocess assets into runtime manifests
";

fn main() -> ExitCode {
    let command = env::args().nth(1);
    match command.as_deref() {
        Some("--help" | "-h" | "help") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(unknown) => {
            eprintln!("unknown command: {unknown}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
