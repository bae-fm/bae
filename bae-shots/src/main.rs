//! bae-shots: takes the shots of the apps and checks them against the design
//! boards.

mod declared;
mod lint;
mod names;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "bae-shots")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// List the shots the harnesses declare that have no board.
    Lint,
}

/// The checkout the command runs in: the nearest directory above the working
/// directory that holds the design system's components.
fn repository_root() -> Result<PathBuf, String> {
    let start = std::env::current_dir().map_err(|e| e.to_string())?;
    start
        .ancestors()
        .find(|d| d.join("design/system/components").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("no design/system/components above {}", start.display()))
}

fn run(action: Action) -> Result<bool, String> {
    let root = repository_root()?;
    match action {
        Action::Lint => {
            let declared = declared::read(&root).map_err(|e| e.to_string())?;
            let problems = lint::problems(&root, &declared);
            for problem in &problems {
                eprintln!("{problem}");
            }
            Ok(problems.is_empty())
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse().action) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("bae-shots: {message}");
            ExitCode::FAILURE
        }
    }
}
