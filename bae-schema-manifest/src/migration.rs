//! The library's migration as a build script reads it: the SQL of every
//! crate's parts in crate order, and the SQL functions and collations that
//! SQL names. A build that checks SQL against the schema applies it to a
//! database of its own, which has none of bae's crates linked.

use crate::{parse, CrateFile, LibraryFile, ManifestError};
use std::path::{Path, PathBuf};

/// A SQL function a crate's `schema.toml` declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredFunction {
    /// The name SQL calls it by.
    pub name: String,
    /// How many arguments it takes.
    pub arguments: u8,
}

/// The library's migration for one kind of device.
#[derive(Debug, PartialEq, Eq)]
pub struct Migration {
    /// The SQL files' texts, in the order they run.
    pub sql: Vec<String>,
    /// The SQL functions the crates declare that this device has.
    pub functions: Vec<DeclaredFunction>,
    /// The names of the collations the crates declare.
    pub collations: Vec<String>,
    /// Every file the migration was read from, relative to the workspace.
    pub files: Vec<PathBuf>,
}

/// Which crates' parts make the library's migration.
#[derive(Debug, PartialEq, Eq)]
pub struct Members {
    /// Every crate of the migration, in order.
    pub crates: Vec<String>,
    /// The crates a phone's library leaves out.
    pub desktop_only: Vec<String>,
}

impl Members {
    /// Whether `crate_name` belongs to the migration of a phone's library, or
    /// is no member and so is built for every device.
    pub fn on_phone(&self, crate_name: &str) -> bool {
        !self.desktop_only.iter().any(|name| name == crate_name)
    }
}

/// How the migration's files are read: the text of the file at a path
/// relative to the workspace.
pub type Reader<'a> = &'a dyn Fn(&Path) -> Result<String, ManifestError>;

/// The members of the library's migration, from bae-library's `schema.toml`.
pub fn members(reader: Reader<'_>) -> Result<Members, ManifestError> {
    let file = PathBuf::from("bae-library/schema.toml");
    let manifest: LibraryFile = parse("bae-library/schema.toml", &reader(&file)?)?;
    Ok(Members { crates: manifest.crates, desktop_only: manifest.desktop_only })
}

/// The migration of a desktop's library when `desktop` holds, and of a
/// phone's otherwise. A function the crate marks desktop-only exists only on
/// a desktop.
pub fn migration(reader: Reader<'_>, desktop: bool) -> Result<Migration, ManifestError> {
    let members = members(reader)?;
    let mut migration = Migration {
        sql: Vec::new(),
        functions: Vec::new(),
        collations: Vec::new(),
        files: vec![PathBuf::from("bae-library/schema.toml")],
    };
    // bae-store declares the functions every crate's SQL calls and holds no
    // table, so its `schema.toml` is read first.
    let crates = std::iter::once(&"bae-store".to_owned())
        .chain(members.crates.iter().filter(|name| desktop || members.on_phone(name)))
        .cloned()
        .collect::<Vec<_>>();
    for name in crates {
        let toml_path = PathBuf::from(&name).join("schema.toml");
        let text = reader(&toml_path)?;
        let manifest: CrateFile = parse(&toml_path.to_string_lossy(), &text)?;
        migration.files.push(toml_path);
        for function in manifest.functions.iter().filter(|function| desktop || !function.desktop) {
            migration.functions.push(DeclaredFunction {
                name: function.name.clone(),
                arguments: function.arguments,
            });
        }
        migration.collations.extend(manifest.collations.iter().map(|c| c.name.clone()));
        for part in &manifest.parts {
            for file in &part.sql {
                let path = PathBuf::from(&name).join(file);
                migration.sql.push(reader(&path)?);
                migration.files.push(path);
            }
        }
    }
    Ok(migration)
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
