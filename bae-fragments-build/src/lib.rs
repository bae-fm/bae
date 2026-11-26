//! The build step of a crate that writes fragments (`sqlive-compose`): it
//! checks the crate's `fragment!` and `read!` calls against the library's
//! schema, writes the Rust they include, hands the crate's fragments to the
//! crates that spread them, and refuses a call that reads a table another
//! crate owns.
//!
//! The schema is every crate's SQL in migration order, on top of coven's
//! readable objects, as a library's migration makes it. A crate that a
//! phone's library holds is checked against the phone's schema too.

mod calls;
mod coven_objects;
mod domains;
mod sql_tables;

use bae_schema_manifest::{members, migration, ManifestError, Migration};
use sqlive_compose::compiler::{compile_crate, Function, Input, Output, Setup};
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

fn run() -> Result<(), String> {
    let crate_name = std::env::var("CARGO_PKG_NAME").map_err(|error| error.to_string())?;
    let crate_dir = environment("CARGO_MANIFEST_DIR")?;
    let out_dir = environment("OUT_DIR")?;
    let root = crate_dir.parent().ok_or("the crate is in the workspace")?.to_path_buf();
    let workspace = |path: &Path| read_file(&root.join(path));

    let policy_path = root.join("owner-construction-check.toml");
    let owners =
        domains::TableOwners::from_policy(&read_file(&policy_path).map_err(|e| e.to_string())?)?;
    let members = members(&workspace).map_err(|error| error.to_string())?;
    let coven = coven_objects::coven_objects();

    let mut manifests = Vec::new();
    for (_, path) in fragment_manifest_variables() {
        println!("cargo::rerun-if-changed={path}");
        manifests.push(read_file(Path::new(&path)).map_err(|error| error.to_string())?);
    }

    let mut output = None;
    let mut schema_files: Vec<PathBuf> = Vec::new();
    let mut failures = Vec::new();
    for desktop in [true, false] {
        if !desktop && !members.on_phone(&crate_name) {
            continue;
        }
        let migration = migration(&workspace, desktop).map_err(|error| error.to_string())?;
        schema_files.extend(migration.files.iter().cloned());
        let setup = setup_of(&coven, &migration);
        // The compiler tells a missing file from another failure by the error's kind.
        let source_reader = |path: &str| read_text(&crate_dir.join(path));
        let compiled = compile_crate(Input {
            crate_name: crate_name.clone(),
            source_reader: &source_reader,
            manifests: manifests.clone(),
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

    let foreign = foreign_reads(&crate_name, &crate_dir, &owners, &output)?;
    if !foreign.is_empty() {
        return Err(foreign.join("\n"));
    }

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
    println!("cargo::rerun-if-changed={}", policy_path.display());
    for warning in &output.warnings {
        println!("cargo::warning={warning}");
    }
    println!("cargo::metadata=sqlive_fragments={}", manifest.display());
    Ok(())
}

/// The setup the compiler runs: coven's objects, then the migration's SQL.
fn setup_of(coven: &coven_objects::CovenObjects, migration: &Migration) -> Setup {
    let mut sql = vec![coven.sql.clone()];
    sql.extend(migration.sql.iter().cloned());
    let mut functions = coven.functions.clone();
    functions.extend(migration.functions.iter().map(|function| Function {
        name: function.name.clone(),
        arguments: i32::from(function.arguments),
        deterministic: true,
    }));
    Setup { sql, functions, collations: migration.collations.clone() }
}

/// Each call of the crate's sources that reads a table another crate owns,
/// as a line of text.
fn foreign_reads(
    crate_name: &str,
    crate_dir: &Path,
    owners: &domains::TableOwners,
    output: &Output,
) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for path in &output.read {
        let text = read_file(&crate_dir.join(path)).map_err(|error| error.to_string())?;
        lines.extend(foreign_reads_in(crate_name, owners, path, &text)?);
    }
    Ok(lines)
}

/// Each call of the source `text` of the file `path` that reads a table
/// another crate than `crate_name` owns, as a line of text.
fn foreign_reads_in(
    crate_name: &str,
    owners: &domains::TableOwners,
    path: &str,
    text: &str,
) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for call in calls::calls_of(text).map_err(|error| format!("{path}: {error}"))? {
        let tables = sql_tables::tables_read(&call.sql);
        for read in owners.foreign_reads(crate_name, tables) {
            lines.push(format!(
                "{crate_name}/{path}:{}: {}! {} reads {}, a table of {} ({}); read it through a fragment of {}",
                call.line, call.macro_name, call.name, read.table, read.domain, read.owner, read.owner
            ));
        }
    }
    Ok(lines)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
