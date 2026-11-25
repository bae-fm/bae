//! bae's schema as data: each crate's `schema.toml` names its SQL files, SQL
//! functions, collations and hooks, and bae-library's names the crates whose
//! parts make the migration. A crate's `build.rs` calls [`emit_crate`] or
//! [`emit_library`], which write the Rust the crate includes from `OUT_DIR`:
//! `SCHEMA_PARTS` and the function and collation statics, or `SCHEMA`.
//!
//! The files are read here, without linking bae's crates, because every
//! domain crate depends on bae-store and bae-library on every domain crate.
//! The generated text names each path relative to its crate, since a build
//! folder cloned from another checkout resolves an absolute path to the
//! checkout it was cloned from (see `bae-desktop-cfg`).

use serde::Deserialize;

/// Why a `schema.toml` produced no Rust.
#[derive(Debug, PartialEq, Eq)]
pub struct ManifestError(String);

impl std::fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ManifestError {}

/// One crate's `schema.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CrateFile {
    #[serde(default, rename = "function")]
    functions: Vec<Function>,
    #[serde(default, rename = "collation")]
    collations: Vec<Collation>,
    #[serde(default, rename = "part")]
    parts: Vec<Part>,
}

/// A SQL function whose body this crate defines.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Function {
    /// The static the function is declared as.
    #[serde(rename = "static")]
    static_name: String,
    /// The name SQL calls it by.
    name: String,
    arguments: u8,
    version: u32,
    /// The Rust path, from the crate root, of the function that computes it.
    compute: String,
    /// The body exists only where `cfg(desktop)` holds.
    #[serde(default)]
    desktop: bool,
    /// Other crates name the static.
    #[serde(default)]
    public: bool,
}

/// A collation whose comparison this crate defines.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Collation {
    #[serde(rename = "static")]
    static_name: String,
    name: String,
    version: u32,
    /// The Rust path, from the crate root, of the comparison.
    compare: String,
}

/// The Rust paths of the functions listing a part's synced tables and its
/// namespaces.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Synced {
    tables: String,
    namespaces: String,
}

/// One domain's share of the schema.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    /// The SQL files, relative to the crate root, in the order they run.
    sql: Vec<String>,
    /// The Rust paths of the statics of the functions the SQL names.
    #[serde(default)]
    functions: Vec<String>,
    /// The Rust paths of the statics of the collations the SQL names.
    #[serde(default)]
    collations: Vec<String>,
    synced: Option<Synced>,
    first_write: Option<String>,
    joining_write: Option<String>,
    before_commit: Option<String>,
}

/// bae-library's `schema.toml`: the crates whose parts make the migration, in
/// order.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    crates: Vec<String>,
    /// The crates of `crates` a phone's library leaves out.
    #[serde(default)]
    desktop_only: Vec<String>,
}

fn parse<T: for<'de> Deserialize<'de>>(file: &str, text: &str) -> Result<T, ManifestError> {
    toml::from_str(text).map_err(|error| ManifestError(format!("{file}: {error}")))
}

/// The Rust the crate `crate_name` includes for its `schema.toml`'s `text`.
pub fn crate_schema(crate_name: &str, text: &str) -> Result<String, ManifestError> {
    let file = format!("{crate_name}/schema.toml");
    let manifest: CrateFile = parse(&file, text)?;
    let store = if crate_name == "bae-store" { "crate" } else { "bae_store" };
    let mut out = String::new();
    for function in &manifest.functions {
        write_function(&mut out, store, function);
    }
    for collation in &manifest.collations {
        write_collation(&mut out, store, collation);
    }
    if !manifest.parts.is_empty() {
        write_parts(&mut out, &file, &manifest)?;
    }
    Ok(out)
}

/// Add `text` to `out` as a line.
fn push_line(out: &mut String, text: String) {
    out.push_str(&text);
    out.push('\n');
}

fn visibility(public: bool) -> &'static str {
    if public {
        "pub"
    } else {
        "pub(crate)"
    }
}

fn write_function(out: &mut String, store: &str, function: &Function) {
    if function.desktop {
        out.push_str("#[cfg(desktop)]\n");
    }
    let Function { static_name, name, arguments, version, compute, .. } = function;
    let visibility = visibility(function.public);
    push_line(out, format!("{visibility} static {static_name}: {store}::SchemaFunction = {store}::SchemaFunction {{ \
         name: {name:?}, arguments: {arguments}, version: {version}, compute: {compute} }};"));
}

fn write_collation(out: &mut String, store: &str, collation: &Collation) {
    let Collation { static_name, name, version, compare } = collation;
    push_line(
        out,
        format!(
            "pub static {static_name}: {store}::SchemaCollation = {store}::SchemaCollation {{ \
         name: {name:?}, version: {version}, compare: {compare} }};"
        ),
    );
}

/// The statics of `manifest` that exist only on desktop, by name.
fn desktop_statics(manifest: &CrateFile) -> Vec<&str> {
    manifest
        .functions
        .iter()
        .filter(|function| function.desktop)
        .map(|function| function.static_name.as_str())
        .collect()
}

fn write_parts(out: &mut String, file: &str, manifest: &CrateFile) -> Result<(), ManifestError> {
    let desktop = desktop_statics(manifest);
    // A const list of references to statics is a named const: a temporary
    // inside a const expression does not live for `'static`.
    let mut lists = String::new();
    let mut parts = String::new();
    for (index, part) in manifest.parts.iter().enumerate() {
        let number = index + 1;
        let part_name = format!("{file}: [[part]] number {number}");
        if part.sql.is_empty() {
            return Err(ManifestError(format!("{part_name}: `sql` names no file")));
        }
        parts.push_str("bae_store::SchemaPart::new(&[\n");
        for sql in &part.sql {
            push_line(
                &mut parts,
                format!("include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/\", {sql:?})),"),
            );
        }
        parts.push_str("])\n");
        for (method, kind, references) in [
            ("functions", "FUNCTIONS", &part.functions),
            ("collations", "COLLATIONS", &part.collations),
        ] {
            if references.is_empty() {
                continue;
            }
            let item = if kind == "FUNCTIONS" { "SchemaFunction" } else { "SchemaCollation" };
            push_line(
                &mut lists,
                format!("const PART_{number}_{kind}: &[&bae_store::{item}] = &["),
            );
            for reference in references {
                let last = reference.rsplit_once("::").map_or(reference.as_str(), |(_, last)| last);
                if desktop.contains(&last) && reference.starts_with("crate::") {
                    lists.push_str("#[cfg(desktop)]\n");
                }
                push_line(&mut lists, format!("&{reference},"));
            }
            lists.push_str("];\n");
            push_line(&mut parts, format!(".{method}(PART_{number}_{kind})"));
        }
        if let Some(Synced { tables, namespaces }) = &part.synced {
            push_line(&mut parts, format!(".synced({tables}, {namespaces})"));
        }
        for (method, path) in [
            ("first_write", &part.first_write),
            ("joining_write", &part.joining_write),
            ("before_commit", &part.before_commit),
        ] {
            if let Some(path) = path {
                push_line(&mut parts, format!(".{method}({path})"));
            }
        }
        parts.push_str(",\n");
    }
    out.push_str(&lists);
    out.push_str("pub const SCHEMA_PARTS: &[bae_store::SchemaPart] = &[\n");
    out.push_str(&parts);
    out.push_str("];\n");
    Ok(())
}

/// The Rust bae-library includes for its `schema.toml`'s `text`: `SCHEMA`,
/// the migration's parts in crate order.
pub fn library_schema(text: &str) -> Result<String, ManifestError> {
    let file = "bae-library/schema.toml";
    let manifest: LibraryFile = parse(file, text)?;
    if let Some(unlisted) =
        manifest.desktop_only.iter().find(|name| !manifest.crates.contains(name))
    {
        return Err(ManifestError(format!(
            "{file}: `desktop_only` names {unlisted}, which `crates` does not list"
        )));
    }
    let mut out = String::from("pub const SCHEMA: bae_store::Schema = bae_store::Schema::new(&[\n");
    for name in &manifest.crates {
        if manifest.desktop_only.contains(name) {
            out.push_str("#[cfg(desktop)]\n");
        }
        push_line(&mut out, format!("{}::SCHEMA_PARTS,", name.replace('-', "_")));
    }
    out.push_str("]);\n");
    Ok(out)
}

#[expect(
    clippy::disallowed_methods,
    reason = "file system: a build script reads the crate's schema.toml and writes its generated Rust, which no library's file reaches"
)]
fn emit(generate: impl FnOnce(&str) -> Result<String, ManifestError>) {
    let crate_dir = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"),
    );
    let out_dir =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let file = crate_dir.join("schema.toml");
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
    let rust = generate(&text).unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(out_dir.join("schema.rs"), rust)
        .unwrap_or_else(|error| panic!("write schema.rs: {error}"));
    println!("cargo::rerun-if-changed=schema.toml");
}

/// The build script of a domain crate: write `OUT_DIR/schema.rs` from the
/// crate's `schema.toml`.
pub fn emit_crate() {
    let crate_name = std::env::var("CARGO_PKG_NAME").expect("cargo sets CARGO_PKG_NAME");
    emit(|text| crate_schema(&crate_name, text));
}

/// The build script of bae-library: write `OUT_DIR/schema.rs` from the
/// crate's `schema.toml`.
pub fn emit_library() {
    emit(library_schema);
}

#[cfg(test)]
mod tests;
