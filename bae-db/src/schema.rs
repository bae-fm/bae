//! bae's store as every handle opens it: its page cache, the synced tables
//! and kinds of file `super::synced` declares, the schema it migrates to, and
//! the collation and SQL functions the schema names.

/// The most each connection to a library keeps in its page cache. A cache
/// holds only pages its connection has read, so a small library stays below
/// it; a large one keeps more of its indexes in memory, which a library-sized
/// play's fill writes to across their whole width. Phones get less: coven
/// opens six connections to a library, each of which may fill its cache.
#[cfg(desktop)]
const PAGE_CACHE_KIB: u32 = 64 * 1024;
#[cfg(not(desktop))]
const PAGE_CACHE_KIB: u32 = 16 * 1024;

/// One domain's share of the schema: its SQL files, which run in the order
/// given.
pub struct SchemaPart {
    sql: &'static [&'static str],
}

impl SchemaPart {
    pub const fn new(sql: &'static [&'static str]) -> Self {
        Self { sql }
    }
}

/// The schema a store migrates to: each crate's parts, in crate order. They
/// run in that order as one migration, so a trigger's part runs after the
/// part creating the table it fires on; a view's or a trigger's body may name
/// a table a later part creates.
#[derive(Clone, Copy)]
pub struct Schema {
    parts: &'static [&'static [SchemaPart]],
}

impl Schema {
    pub const fn new(parts: &'static [&'static [SchemaPart]]) -> Self {
        Self { parts }
    }

    /// Every SQL file of the schema, in the order they run.
    pub fn sql(self) -> impl Iterator<Item = &'static str> {
        self.parts.iter().flat_map(|parts| parts.iter()).flat_map(|part| part.sql.iter().copied())
    }

    /// The store's migrations: the schema as migration 1, each SQL file run in
    /// order inside coven's migration transaction.
    pub fn migrations(self) -> Vec<coven::Migration> {
        vec![coven::Migration::addition_with(1, "initial", move |context| {
            for sql in self.sql() {
                context.execute_batch(sql)?;
            }
            Ok(())
        })]
    }
}

/// bae's store as every handle opens it: its page cache size, its synced
/// tables, `schema`, and the collation and SQL functions the schema names.
pub fn configure_store(builder: coven::CovenBuilder, schema: Schema) -> coven::CovenBuilder {
    let builder = builder
        .sqlite_settings(coven::SqliteSettings {
            cache_size: Some(coven::CacheSize::Kibibytes(PAGE_CACHE_KIB)),
            ..Default::default()
        })
        .synced_tables(super::synced::synced_tables())
        .namespaces(super::synced::namespaces())
        .migrations(schema.migrations())
        .collation(
            super::collation::NATURAL_ORDER,
            super::collation::NATURAL_ORDER_VERSION,
            super::collation::natural_order,
        );
    register_functions(builder)
}

/// Declares the SQL functions the schema names once, as the registration on
/// coven's builder (which every handle opens with) and as the registration
/// on a bare connection (`bare_library_connection`): `name, arguments,
/// version, |args| body`, where `body` reads its arguments with `args.get(n)`.
macro_rules! schema_functions {
    ($($name:literal, $arguments:literal, $version:literal, |$args:ident| $body:expr;)*) => {
        fn register_functions(builder: coven::CovenBuilder) -> coven::CovenBuilder {
            builder$(.function(
                $name,
                $arguments,
                coven::Determinism::Deterministic { version: $version },
                |$args| $body,
            ))*
        }

        #[cfg(any(test, feature = "test-utils"))]
        fn register_functions_on(conn: &rusqlite::Connection) {
            use rusqlite::functions::FunctionFlags;
            $(
                conn.create_scalar_function(
                    $name,
                    $arguments,
                    FunctionFlags::SQLITE_UTF8
                        | FunctionFlags::SQLITE_DETERMINISTIC
                        | FunctionFlags::SQLITE_INNOCUOUS,
                    |$args| $body,
                )
                .expect(concat!($name, " registers"));
            )*
        }
    };
}

schema_functions! {
    "bae_fold_case", 1, 1, |arguments| Ok(fold_case_value(arguments.get(0)?));
    "bae_search_pattern", 1, 1, |arguments| Ok(search_pattern_value(arguments.get(0)?));
    "bae_catalog_rank", 1, 1, |arguments| catalog_rank_value(arguments.get(0)?);
    "bae_folder_below", 2, 2, |arguments| folder_below_value(arguments.get(0)?, arguments.get(1)?);
    "bae_folder_relative", 2, 2, |arguments| {
        folder_relative_value(arguments.get(0)?, arguments.get(1)?)
    };
}

/// `bae_fold_case`: a text folded as library search folds it; NULL stays NULL.
fn fold_case_value(text: Option<String>) -> Option<String> {
    text.map(|text| fold_case(&text))
}

/// `bae_search_pattern`: the `GLOB` pattern library search matches a query
/// by; NULL stays NULL.
fn search_pattern_value(query: Option<String>) -> Option<String> {
    query.map(|query| search_pattern(&query))
}

/// `bae_catalog_rank`: a stored catalog's place in the order surfaces list
/// catalogs (`Catalog::ALL`). Part of the schema once an index uses it: the
/// order must not change.
fn catalog_rank_value(catalog: String) -> rusqlite::Result<i64> {
    let catalog: bae_catalogs::catalog::Catalog = catalog
        .parse()
        .map_err(|error: String| rusqlite::Error::UserFunctionError(error.into()))?;
    Ok(catalog.rank() as i64)
}

/// `bae_folder_below`: the absolute path's bytes of a folder below a watched
/// folder, from the root's bytes and the folder's [`RelativeFolder`] bytes,
/// spelled as a scan spells it, which is how a stored reading names the
/// folder it reads. Takes BLOBs only: rusqlite refuses TEXT for a `Vec<u8>`,
/// so a path bound as text fails rather than matching nothing.
///
/// [`RelativeFolder`]: crate::RelativeFolder
fn folder_below_value(root: Vec<u8>, relative: Vec<u8>) -> rusqlite::Result<Vec<u8>> {
    let root = coven::path_from_bytes(root)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
    let relative = crate::RelativeFolder::from_bytes(relative)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
    Ok(coven::path_bytes(&crate::watched_path::folder_below(&root, &relative)))
}

/// `bae_folder_relative`: the [`RelativeFolder`] bytes of a folder below a
/// watched folder, empty for the watched folder itself, NULL for one outside
/// it. Takes BLOBs only, as `bae_folder_below` does.
///
/// [`RelativeFolder`]: crate::RelativeFolder
fn folder_relative_value(root: Vec<u8>, folder: Vec<u8>) -> rusqlite::Result<Option<Vec<u8>>> {
    let to_error =
        |error: coven::PathBytesError| rusqlite::Error::UserFunctionError(Box::new(error));
    let root = coven::path_from_bytes(root).map_err(to_error)?;
    let folder = coven::path_from_bytes(folder).map_err(to_error)?;
    crate::watched_path::folder_relative_below(&root, &folder)
        .map(|relative| relative.map(|relative| relative.as_bytes().to_vec()))
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
}

/// An in-memory database on `schema` outside coven, with the SQL functions
/// the schema calls: for unit tests of SQL that runs on a bare connection.
#[cfg(any(test, feature = "test-utils"))]
pub fn bare_library_connection(schema: Schema) -> rusqlite::Connection {
    use rusqlite::functions::FunctionFlags;
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    register_functions_on(&conn);
    let deterministic = FunctionFlags::SQLITE_UTF8
        | FunctionFlags::SQLITE_DETERMINISTIC
        | FunctionFlags::SQLITE_INNOCUOUS;
    // coven registers `coven_path_text` on every connection it opens; this one
    // shows a path's bytes the same way, through coven's exports.
    conn.create_scalar_function("coven_path_text", 1, deterministic, |context| {
        context
            .get::<Option<Vec<u8>>>(0)?
            .map(|bytes| {
                coven::path_from_bytes(bytes)
                    .map(|path| coven::path_text(&path))
                    .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
            })
            .transpose()
    })
    .expect("coven_path_text registers");
    conn.create_collation(super::collation::NATURAL_ORDER, super::collation::natural_order)
        .expect("natural_order registers");
    // The schema's triggers follow coven's `coven_files`, `coven_pins`,
    // `coven_upload_queue` and `coven_sync_status`, which coven creates
    // before it migrates; these tests pin, queue and sync nothing, so tables
    // of the same shape let the triggers exist, and a test whose rows
    // reference a file writes the file's stand-in record itself.
    conn.execute_batch(
        "CREATE TABLE coven_files (id TEXT PRIMARY KEY NOT NULL, audience TEXT NOT NULL, \
         size INTEGER NOT NULL, hash BLOB, coordinates TEXT NOT NULL, \
         namespace TEXT NOT NULL, shares TEXT) STRICT, WITHOUT ROWID; \
         CREATE TABLE coven_pins (file TEXT PRIMARY KEY NOT NULL) STRICT, WITHOUT ROWID; \
         CREATE TABLE coven_upload_queue (file TEXT PRIMARY KEY NOT NULL, \
         position INTEGER NOT NULL) STRICT, WITHOUT ROWID; \
         CREATE TABLE coven_sync_status (singleton INTEGER PRIMARY KEY NOT NULL, \
         storage TEXT NOT NULL, provider TEXT, last_pass BLOB, moving INTEGER NOT NULL, \
         last_synced INTEGER, starts INTEGER NOT NULL) STRICT;",
    )
    .expect("the stand-in coven tables create");
    for sql in schema.sql() {
        conn.execute_batch(sql).expect("the schema applies");
    }
    conn
}

/// A text as library search compares it: lowercased by Unicode's rules. The
/// schema's `bae_fold_case` folds each searched title and name this way, and
/// a query is folded the same way before it is matched against them.
pub(crate) fn fold_case(text: &str) -> String {
    text.to_lowercase()
}

/// The `GLOB` pattern matching `query` anywhere inside a text folded as the
/// search tables fold theirs: the folded query with `*`, `?` and `[`, which
/// `GLOB` reads as wildcards, each written as the one-character class that
/// matches only itself. The schema's `bae_search_pattern` is this, for the
/// import list's search.
pub fn search_pattern(query: &str) -> String {
    let mut pattern = String::from("*");
    for character in fold_case(query).chars() {
        match character {
            '*' => pattern.push_str("[*]"),
            '?' => pattern.push_str("[?]"),
            '[' => pattern.push_str("[[]"),
            other => pattern.push(other),
        }
    }
    pattern.push('*');
    pattern
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
