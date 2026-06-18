//! `loc-gen` — validate the master catalog and emit native resource files.
//!
//!   loc-gen check  [--catalog <path>]
//!   loc-gen emit   --target {apple|android|windows} --out-dir <dir> [--catalog <path>]
//!
//! `--catalog` defaults to `bae-bridge/loc/catalog.toml` (relative to the
//! working directory, i.e. the repo root the build scripts run from).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bae_loc::{check, emit, Catalog};

const SOURCE_LANGUAGE: &str = "en";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("loc-gen: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    subcommand: String,
    catalog: PathBuf,
    target: Option<String>,
    out_dir: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut subcommand = None;
    let mut catalog = PathBuf::from("bae-bridge/loc/catalog.toml");
    let mut target = None;
    let mut out_dir = None;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "check" | "emit" => subcommand = Some(raw[i].clone()),
            "--catalog" => catalog = PathBuf::from(value(&raw, &mut i)?),
            "--target" => target = Some(value(&raw, &mut i)?.to_string()),
            "--out-dir" => out_dir = Some(PathBuf::from(value(&raw, &mut i)?)),
            other => return Err(format!("unexpected argument `{other}`")),
        }
        i += 1;
    }
    let subcommand = subcommand.ok_or("expected a subcommand: `check` or `emit`")?;
    Ok(Args {
        subcommand,
        catalog,
        target,
        out_dir,
    })
}

fn value<'a>(raw: &'a [String], i: &mut usize) -> Result<&'a str, String> {
    *i += 1;
    raw.get(*i)
        .map(String::as_str)
        .ok_or_else(|| format!("`{}` needs a value", raw[*i - 1]))
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let src = fs::read_to_string(&args.catalog)
        .map_err(|e| format!("reading {}: {e}", args.catalog.display()))?;
    let catalog = Catalog::from_toml(&src).map_err(|e| format!("parsing catalog: {e}"))?;

    // Both subcommands validate; emit must not write a broken catalog out.
    if let Err(errs) = check::validate(&catalog) {
        return Err(format!(
            "{} catalog problem(s):\n  - {}",
            errs.len(),
            errs.join("\n  - ")
        ));
    }

    match args.subcommand.as_str() {
        "check" => {
            println!("loc-gen: {} messages, catalog ok", catalog.messages.len());
            Ok(())
        }
        "emit" => emit_target(&args, &catalog),
        other => Err(format!("unknown subcommand `{other}`")),
    }
}

fn emit_target(args: &Args, catalog: &Catalog) -> Result<(), String> {
    let target = args.target.as_deref().ok_or("emit needs --target")?;
    let out_dir = args.out_dir.as_ref().ok_or("emit needs --out-dir")?;

    let (relative, contents) = match target {
        "apple" => (
            PathBuf::from("Core.xcstrings"),
            emit::apple_xcstrings(catalog, SOURCE_LANGUAGE)?,
        ),
        "android" => (
            PathBuf::from("values/core_strings.xml"),
            emit::android_strings_xml(catalog),
        ),
        "windows" => (
            PathBuf::from(format!("{SOURCE_LANGUAGE}-US/Core.resw")),
            emit::windows_resw(catalog),
        ),
        other => {
            return Err(format!(
                "unknown --target `{other}` (apple|android|windows)"
            ))
        }
    };

    let path = out_dir.join(relative);
    write_file(&path, &contents)?;
    println!("loc-gen: wrote {}", path.display());
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("writing {}: {e}", path.display()))
}
