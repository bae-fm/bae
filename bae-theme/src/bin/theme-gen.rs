//! `theme-gen emit --target {apple|android} --out-dir <dir> [--theme <path>]`
//!
//! Writes the platform's generated theme under `<dir>`. `--theme` defaults to
//! `design/theme.toml`, relative to the repo root the build scripts run from.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bae_theme::{android, apple, Theme};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("theme-gen: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut theme_path = PathBuf::from("design/theme.toml");
    let mut target = None;
    let mut out_dir = None;
    let mut emit = false;
    let mut args = raw.iter();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .cloned()
                .ok_or_else(|| format!("`{arg}` needs a value"))
        };
        match arg.as_str() {
            "emit" => emit = true,
            "--theme" => theme_path = PathBuf::from(value()?),
            "--target" => target = Some(value()?),
            "--out-dir" => out_dir = Some(PathBuf::from(value()?)),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    if !emit {
        return Err("expected the `emit` subcommand".to_string());
    }
    let target = target.ok_or("emit needs --target")?;
    let out_dir = out_dir.ok_or("emit needs --out-dir")?;

    let source = fs::read_to_string(&theme_path)
        .map_err(|error| format!("reading {}: {error}", theme_path.display()))?;
    let theme = Theme::from_toml(&source).map_err(|problems| {
        format!(
            "{} problem(s) in {}:\n  - {}",
            problems.len(),
            theme_path.display(),
            problems.join("\n  - ")
        )
    })?;

    let files: Vec<(String, String)> = match target.as_str() {
        "apple" => vec![(apple::FILE.to_string(), apple::swift(&theme))],
        "android" => {
            let mut files = vec![(android::KOTLIN_FILE.to_string(), android::kotlin(&theme))];
            files.extend(android::launch_resources(&theme)?);
            files
        }
        other => return Err(format!("unknown --target `{other}` (apple|android)")),
    };
    for (relative, contents) in files {
        write_if_changed(&out_dir.join(relative), &contents)?;
    }
    Ok(())
}

/// Leaves an unchanged file's timestamp alone, so the platform build does not
/// recompile what did not change.
fn write_if_changed(path: &Path, contents: &str) -> Result<(), String> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("creating {}: {error}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|error| format!("writing {}: {error}", path.display()))
}
