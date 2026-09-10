//! `loc-gen` — validate the master catalog, emit native resource files, and
//! print the shipping locale set.
//!
//!   loc-gen check   [--catalog &lt;path&gt;]
//!   loc-gen emit    --target {apple|android|resx} --out-dir &lt;dir&gt; [--catalog &lt;path&gt;]
//!   loc-gen locales
//!
//! `locales` writes `bae_loc::TARGET_LOCALES`, one per line, so the Python
//! catalog gates (`scripts/check_localizable_strings.py`,
//! `scripts/loc-english-skeleton.py`) read the set from the crate that declares
//! it instead of keeping a copy.
//!
//! `--catalog` defaults to `bae-bridge/loc/catalog.toml` (relative to the
//! working directory, i.e. the repo root the build scripts run from).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bae_loc::{check, emit, Catalog, TARGET_LOCALES};

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
            "check" | "emit" | "locales" => subcommand = Some(raw[i].clone()),
            "--catalog" => catalog = PathBuf::from(value(&raw, &mut i)?),
            "--target" => target = Some(value(&raw, &mut i)?.to_string()),
            "--out-dir" => out_dir = Some(PathBuf::from(value(&raw, &mut i)?)),
            other => return Err(format!("unexpected argument `{other}`")),
        }
        i += 1;
    }
    let subcommand = subcommand.ok_or("expected a subcommand: `check`, `emit` or `locales`")?;
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

    // The locale set is a compile-time constant; printing it must not depend on
    // a catalog that parses, or the gates that read it can't run on a catalog
    // they exist to diagnose.
    if args.subcommand == "locales" {
        for locale in TARGET_LOCALES {
            println!("{locale}");
        }
        return Ok(());
    }

    let src = fs::read_to_string(&args.catalog)
        .map_err(|e| format!("reading {}: {e}", args.catalog.display()))?;
    let catalog = Catalog::from_toml(&src).map_err(|e| format!("parsing catalog: {e}"))?;

    // `check` and `emit` both validate each message; emit must not write a
    // broken catalog out.
    if let Err(errs) = check::validate(&catalog) {
        return Err(format!(
            "{} catalog problem(s):\n  - {}",
            errs.len(),
            errs.join("\n  - ")
        ));
    }

    match args.subcommand.as_str() {
        "check" => {
            // Locale coverage gates `check` alone. `emit` fills a gap with the
            // English source at state `new`, which is what lets a locale be
            // added to `TARGET_LOCALES` and translated catalog by catalog; this
            // is the gate that says the translating is finished.
            if let Err(errs) = check::locale_coverage(&catalog) {
                return Err(format!(
                    "{} locale coverage problem(s):\n  - {}",
                    errs.len(),
                    errs.join("\n  - ")
                ));
            }
            println!(
                "loc-gen: {} messages, {} locales, catalog ok",
                catalog.messages.len(),
                TARGET_LOCALES.len()
            );
            Ok(())
        }
        "emit" => emit_target(&args, &catalog),
        other => Err(format!("unknown subcommand `{other}`")),
    }
}

/// Wrap Apple's one multi-locale file as the one-element set the caller
/// iterates. The other targets already fan out to a file per locale.
fn single_file(path: impl Into<PathBuf>, contents: String) -> Vec<(PathBuf, String)> {
    vec![(path.into(), contents)]
}

fn emit_target(args: &Args, catalog: &Catalog) -> Result<(), String> {
    let target = args.target.as_deref().ok_or("emit needs --target")?;
    let out_dir = args.out_dir.as_ref().ok_or("emit needs --out-dir")?;

    // Android emits a directory per shipping locale.
    let files: Vec<(PathBuf, String)> = match target {
        "apple" => single_file("Core.xcstrings", emit::apple_xcstrings(catalog)?),
        "android" => emit::android_resource_files(catalog)
            .into_iter()
            .map(|(rel, contents)| (PathBuf::from(rel), contents))
            .collect(),
        "resx" => emit::resx_all(catalog),
        other => return Err(format!("unknown --target `{other}` (apple|android|resx)")),
    };

    for (relative, contents) in &files {
        let path = out_dir.join(relative);
        write_file(&path, contents)?;
        println!("loc-gen: wrote {}", path.display());
    }
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("writing {}: {e}", path.display()))
}
