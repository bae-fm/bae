//! bae's store as every handle opens it: its page cache, the schema it
//! migrates to, and the synced tables, collations and SQL functions the
//! schema's parts declare.

mod text;
pub use text::{FOLD_CASE, NATURAL_ORDER, SEARCH_PATTERN};

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
/// given, the SQL functions and collations those files name, and the synced
/// tables and namespaces of its tables.
pub struct SchemaPart {
    sql: &'static [&'static str],
    functions: &'static [&'static SchemaFunction],
    collations: &'static [&'static SchemaCollation],
    synced_tables: fn() -> Vec<coven::SyncedTable>,
    namespaces: fn() -> Vec<coven::Namespace>,
}

fn no_synced_tables() -> Vec<coven::SyncedTable> {
    Vec::new()
}

fn no_namespaces() -> Vec<coven::Namespace> {
    Vec::new()
}

impl SchemaPart {
    pub const fn new(sql: &'static [&'static str]) -> Self {
        Self {
            sql,
            functions: &[],
            collations: &[],
            synced_tables: no_synced_tables,
            namespaces: no_namespaces,
        }
    }

    /// The SQL functions this part's SQL names.
    pub const fn functions(mut self, functions: &'static [&'static SchemaFunction]) -> Self {
        self.functions = functions;
        self
    }

    /// The collations this part's SQL names.
    pub const fn collations(mut self, collations: &'static [&'static SchemaCollation]) -> Self {
        self.collations = collations;
        self
    }

    /// The synced tables and namespaces of this part's tables.
    pub const fn synced(
        mut self,
        tables: fn() -> Vec<coven::SyncedTable>,
        namespaces: fn() -> Vec<coven::Namespace>,
    ) -> Self {
        self.synced_tables = tables;
        self.namespaces = namespaces;
        self
    }
}

/// The arguments one call of a [`SchemaFunction`] reads: coven's on every
/// connection a store opens, and, for tests of SQL on a bare connection,
/// rusqlite's.
pub enum SqlArguments<'a> {
    Store(&'a coven::FunctionArguments<'a>),
    #[cfg(any(test, feature = "test-utils"))]
    Bare(&'a rusqlite::functions::Context<'a>),
}

impl SqlArguments<'_> {
    /// Argument `index`, counted from zero, converted as a row's column is.
    pub fn get<T: rusqlite::types::FromSql>(&self, index: usize) -> rusqlite::Result<T> {
        match self {
            Self::Store(arguments) => arguments.get(index),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Bare(context) => context.get(index),
        }
    }
}

/// A scalar SQL function the schema names. A change to what it computes
/// takes a new `version`, and coven rebuilds the indexes that call it on the
/// next writable open. Declared as a `pub static`: a function is registered
/// once however many parts list it, which is told by its address.
pub struct SchemaFunction {
    pub name: &'static str,
    pub arguments: u8,
    pub version: u32,
    pub compute: fn(&SqlArguments<'_>) -> rusqlite::Result<rusqlite::types::Value>,
}

/// A collation the schema names. Declared as a `pub static`, as
/// [`SchemaFunction`] is.
pub struct SchemaCollation {
    pub name: &'static str,
    pub version: u32,
    pub compare: fn(&str, &str) -> std::cmp::Ordering,
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

    fn parts(self) -> impl Iterator<Item = &'static SchemaPart> {
        self.parts.iter().flat_map(|parts| parts.iter())
    }

    /// Every SQL file of the schema, in the order they run.
    pub fn sql(self) -> impl Iterator<Item = &'static str> {
        self.parts().flat_map(|part| part.sql.iter().copied())
    }

    /// The SQL functions the parts name, each once however many parts list it.
    pub fn functions(self) -> Vec<&'static SchemaFunction> {
        let mut functions: Vec<&'static SchemaFunction> = Vec::new();
        for function in self.parts().flat_map(|part| part.functions.iter().copied()) {
            if !functions.iter().any(|seen| std::ptr::eq(*seen, function)) {
                functions.push(function);
            }
        }
        functions
    }

    /// The collations the parts name, each once however many parts list it.
    pub fn collations(self) -> Vec<&'static SchemaCollation> {
        let mut collations: Vec<&'static SchemaCollation> = Vec::new();
        for collation in self.parts().flat_map(|part| part.collations.iter().copied()) {
            if !collations.iter().any(|seen| std::ptr::eq(*seen, collation)) {
                collations.push(collation);
            }
        }
        collations
    }

    /// The synced tables the parts declare, in part order.
    pub fn synced_tables(self) -> Vec<coven::SyncedTable> {
        self.parts().flat_map(|part| (part.synced_tables)()).collect()
    }

    /// The namespaces the parts declare, in part order.
    pub fn namespaces(self) -> Vec<coven::Namespace> {
        self.parts().flat_map(|part| (part.namespaces)()).collect()
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

/// bae's store as every handle opens it: its page cache size, and the synced
/// tables, namespaces, migrations, collations and SQL functions `schema`'s
/// parts declare.
pub fn configure_store(builder: coven::CovenBuilder, schema: Schema) -> coven::CovenBuilder {
    let mut builder = builder
        .sqlite_settings(coven::SqliteSettings {
            cache_size: Some(coven::CacheSize::Kibibytes(PAGE_CACHE_KIB)),
            ..Default::default()
        })
        .synced_tables(schema.synced_tables())
        .namespaces(schema.namespaces())
        .migrations(schema.migrations());
    for collation in schema.collations() {
        builder = builder.collation(collation.name, collation.version, collation.compare);
    }
    for function in schema.functions() {
        let compute = function.compute;
        builder = builder.function(
            function.name,
            function.arguments,
            coven::Determinism::Deterministic { version: function.version },
            move |arguments| compute(&SqlArguments::Store(arguments)),
        );
    }
    builder
}

/// An in-memory database on `schema` outside coven, with the SQL functions
/// the schema calls: for unit tests of SQL that runs on a bare connection.
#[cfg(any(test, feature = "test-utils"))]
pub fn bare_library_connection(schema: Schema) -> rusqlite::Connection {
    use rusqlite::functions::FunctionFlags;
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
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
    for function in schema.functions() {
        let compute = function.compute;
        conn.create_scalar_function(
            function.name,
            i32::from(function.arguments),
            deterministic,
            move |context| compute(&SqlArguments::Bare(context)),
        )
        .unwrap_or_else(|error| panic!("{} registers: {error}", function.name));
    }
    for collation in schema.collations() {
        conn.create_collation(collation.name, collation.compare)
            .unwrap_or_else(|error| panic!("{} registers: {error}", collation.name));
    }
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
