//! The build step of a crate that writes fragments (`sqlive-compose`): it
//! checks the crate's `fragment!`, `read!` and `write!` calls against the library's
//! schema, writes the Rust they include, hands the crate's fragments to the
//! crates that spread them, and refuses a call that reads a table of a domain
//! whose homes don't hold the call's file.
//!
//! The schema is every crate's SQL in migration order, on top of coven's
//! readable objects, as a library's migration makes it. A crate that a
//! phone's library holds is checked against the phone's schema too.

mod domains;

use bae_schema_manifest::{members, migration, ManifestError, Migration};
use sqlive_compose::compiler::{compile_crate, CallKind, CompiledCall, Function, Input, Setup};
use std::path::{Path, PathBuf};

/// The build script of a crate that writes fragments, or only reads them
/// composed from other crates'. It needs `links` in the crate's manifest, so
/// that the crates which depend on it receive its fragments.
pub fn emit() {
    if let Err(message) = run() {
        panic!("{message}");
    }
}

/// The names of the environment variables through which Cargo gives a build
/// script the fragments of the crates it depends on.
fn fragment_manifest_variables() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(name, _)| name.starts_with("DEP_") && name.ends_with("_SQLIVE_FRAGMENTS"))
        .collect()
}

#[expect(
    clippy::disallowed_methods,
    reason = "file system: a build script reads the workspace's schema files and the crate's sources, and writes what the crate includes"
)]
fn read_text(path: &Path) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

fn read_file(path: &Path) -> Result<String, ManifestError> {
    read_text(path).map_err(|error| ManifestError::unreadable(path, &error))
}

#[expect(
    clippy::disallowed_methods,
    reason = "file system: a build script writes what the crate includes"
)]
fn write_file(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "file system: a build script makes the folder it writes the generated Rust in"
)]
fn make_folder(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("make {}: {error}", path.display()))
}

fn environment(name: &str) -> Result<PathBuf, String> {
    std::env::var_os(name).map(PathBuf::from).ok_or_else(|| format!("cargo sets {name}"))
}

/// What a crate's build step compiled.
struct Checked {
    output: sqlive_compose::compiler::Output,
    /// The schema files the compile read, relative to the workspace.
    schema_files: Vec<PathBuf>,
}

/// Compile the crate `crate_name`, whose folder is `crate_dir` in the
/// workspace `root`, against the library's schema for each kind of device the
/// crate is built for, and refuse a call that reads another domain's table.
/// `manifests` are the fragments of the crates it depends on.
fn check_crate(
    crate_name: &str,
    crate_dir: &Path,
    root: &Path,
    manifests: &[String],
) -> Result<Checked, String> {
    let workspace = |path: &Path| read_file(&root.join(path));
    let policy_text = read_file(&root.join("owner-construction-check.toml"))
        .map_err(|error| error.to_string())?;
    let domains = domains::Domains::from_policy(&policy_text)?;
    let members = members(&workspace).map_err(|error| error.to_string())?;

    let mut output = None;
    let mut schema_files: Vec<PathBuf> = Vec::new();
    let mut failures = Vec::new();
    for desktop in [true, false] {
        if !desktop && !members.on_phone(crate_name) {
            continue;
        }
        let migration = migration(&workspace, desktop).map_err(|error| error.to_string())?;
        schema_files.extend(migration.files.iter().cloned());
        let setup = setup_of(&migration);
        // The compiler tells a missing file from another failure by the error's kind.
        let source_reader = |path: &str| read_text(&crate_dir.join(path));
        let compiled = compile_crate(Input {
            crate_name: crate_name.to_owned(),
            macros_crate: "sqlive_compose".to_owned(),
            source_reader: &source_reader,
            manifests: manifests.to_vec(),
            setup,
        });
        let label = if desktop { "desktop" } else { "phone" };
        match compiled {
            Ok(compiled) if output.is_none() => output = Some(compiled),
            Ok(_) => {}
            Err(errors) => {
                failures.extend(errors.iter().map(|error| format!("{label} schema: {error}")));
            }
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n"));
    }
    let output = output.ok_or("the crate compiled for no schema")?;
    let foreign = foreign_reads(crate_name, &domains, &output.calls);
    if !foreign.is_empty() {
        return Err(foreign.join("\n"));
    }
    Ok(Checked { output, schema_files })
}

fn run() -> Result<(), String> {
    let crate_name = std::env::var("CARGO_PKG_NAME").map_err(|error| error.to_string())?;
    let crate_dir = environment("CARGO_MANIFEST_DIR")?;
    let out_dir = environment("OUT_DIR")?;
    let root = crate_dir.parent().ok_or("the crate is in the workspace")?.to_path_buf();

    let mut manifests = Vec::new();
    for (_, path) in fragment_manifest_variables() {
        println!("cargo::rerun-if-changed={path}");
        manifests.push(read_file(Path::new(&path)).map_err(|error| error.to_string())?);
    }
    let Checked { output, schema_files } = check_crate(&crate_name, &crate_dir, &root, &manifests)?;

    let generated = out_dir.join("sqlive_compose");
    make_folder(&generated)?;
    for (name, text) in &output.files {
        write_file(&generated.join(name), text)?;
    }
    let manifest = out_dir.join("fragments.json");
    write_file(&manifest, &output.manifest)?;
    for path in &output.read {
        println!("cargo::rerun-if-changed={}", crate_dir.join(path).display());
    }
    for path in schema_files.iter().collect::<std::collections::BTreeSet<_>>() {
        println!("cargo::rerun-if-changed={}", root.join(path).display());
    }
    println!("cargo::rerun-if-changed={}", root.join("owner-construction-check.toml").display());
    for warning in &output.warnings {
        println!("cargo::warning={warning}");
    }
    println!("cargo::metadata=sqlive_fragments={}", manifest.display());
    Ok(())
}

/// The setup the compiler runs: coven's, then the migration's SQL with the
/// functions and collations it names.
fn setup_of(migration: &Migration) -> Setup {
    let mut setup = coven_schema::compiler_setup();
    setup.sql.extend(migration.sql.iter().cloned());
    setup.functions.extend(migration.functions.iter().map(|function| Function {
        name: function.name.clone(),
        arguments: i32::from(function.arguments),
        deterministic: true,
    }));
    setup.collations.extend(migration.collations.iter().cloned());
    setup
}

/// Each call of the crate that reads a table of a domain whose homes don't hold
/// the call's file, as a line of text.
fn foreign_reads(
    crate_name: &str,
    domains: &domains::Domains,
    calls: &[CompiledCall],
) -> Vec<String> {
    let mut lines = Vec::new();
    for call in calls {
        let macro_name = match call.kind {
            CallKind::Fragment => "fragment",
            CallKind::Read => "read",
            CallKind::Write => "write",
        };
        let file = format!("{crate_name}/{}", call.location.file);
        for read in domains.foreign_reads(&file, call.reads.iter().cloned()) {
            lines.push(format!(
                "{file}:{}: {macro_name}! {} reads {}, a table of the domain {:?}, whose homes \
                 don't hold this file; read it through a fragment of that domain",
                call.location.line, call.name, read.table, read.domain
            ));
        }
    }
    lines
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "wiring_tests.rs"]
mod wiring_tests;
