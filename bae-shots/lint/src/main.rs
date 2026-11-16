//! bae-shots-lint: lists the shots the harnesses declare, and the entries of
//! the record, that have no board. bae-shots-lint fails when it lists any.

use bae_shots_lint::{declared, lint, record, repository_root};
use std::process::ExitCode;

/// True when every declared shot and every record entry has a board.
fn run() -> Result<bool, String> {
    let root = repository_root()?;
    let declared = declared::read(&root).map_err(|e| e.to_string())?;
    let record = record::Record::load(&root.join(record::FILE)).map_err(|e| e.to_string())?;
    let problems = lint::problems(&root, &declared, &record);
    for problem in &problems {
        eprintln!("{}", problem.sentence);
    }
    Ok(problems.is_empty())
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("bae-shots-lint: {message}");
            ExitCode::FAILURE
        }
    }
}
