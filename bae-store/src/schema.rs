//! bae's store as every handle opens it: its page cache, the schema it
//! migrates to, and the synced tables, collations and SQL functions the
//! schema's parts declare.

mod text;
pub use text::{FOLD_CASE, NAME_KEY, NAME_LIST_KEY, NATURAL_ORDER, SEARCH_PATTERN};

/// The most each connection to a library keeps in its page cache. A cache
/// holds only pages its connection has read, so a small library stays below
/// it; a large one keeps more of its indexes in memory, which a library-sized
/// play's fill writes to across their whole width. Phones get less: coven
/// opens six connections to a library, each of which may fill its cache.
#[cfg(desktop)]
const PAGE_CACHE_KIB: u32 = 64 * 1024;
#[cfg(not(desktop))]
const PAGE_CACHE_KIB: u32 = 16 * 1024;

/// A domain's write for a member's device as its registration applies: coven
/// runs it as a local write, in the transaction that first applies the
/// registration, with the facts of the member's latest admission.
pub type JoiningWrite = fn(&coven::JoiningWrite<'_>, &coven::Joining) -> Result<(), coven::DbError>;

/// One domain's share of the schema: its SQL files, which run in the order
/// given, the SQL functions and collations those files name, the synced
/// tables and namespaces of its tables, and its write for a member's device
/// as it registers.
pub struct SchemaPart {
    sql: &'static [&'static str],
    functions: &'static [&'static SchemaFunction],
    collations: &'static [&'static SchemaCollation],
    synced_tables: fn() -> Vec<coven::SyncedTable>,
    namespaces: fn() -> Vec<coven::Namespace>,
    joining_write: Option<JoiningWrite>,
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
            joining_write: None,
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

    /// This part's write for a member's device as its registration applies.
    pub const fn joining_write(mut self, write: JoiningWrite) -> Self {
        self.joining_write = Some(write);
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

    /// Argument `index`, counted from zero, as SQLite holds it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn get_raw(&self, index: usize) -> rusqlite::types::ValueRef<'_> {
        match self {
            Self::Store(arguments) => arguments.get_raw(index),
            Self::Bare(context) => context.get_raw(index),
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

    /// This schema with `sql` run after every part, for a test that needs
    /// the schema changed in a way only SQL makes; coven makes no schema
    /// change outside a migration.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn followed_by(self, sql: String) -> Self {
        let sql: &'static [&'static str] = Box::leak(Box::new([&*Box::leak(sql.into_boxed_str())]));
        let part: &'static [SchemaPart] = Box::leak(Box::new([SchemaPart::new(sql)]));
        let mut parts = self.parts.to_vec();
        parts.push(part);
        Self { parts: Box::leak(parts.into_boxed_slice()) }
    }

    /// The parts' writes for a member's device as its registration applies,
    /// in part order.
    pub fn joining_writes(self) -> Vec<JoiningWrite> {
        self.parts().filter_map(|part| part.joining_write).collect()
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
    let joining_writes = schema.joining_writes();
    if !joining_writes.is_empty() {
        builder = builder.joining_write(move |sql, joining| {
            joining_writes.iter().try_for_each(|write| write(sql, joining))
        });
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

#[cfg(any(test, feature = "test-utils"))]
impl Schema {
    /// Open the library on `conn`, a fresh in-memory connection the caller
    /// opened, outside coven: the schema's functions and collations (and the
    /// `coven_path_text` coven registers on every connection it opens) are
    /// registered, then the tables coven lets the triggers follow, made as
    /// coven makes them, and the schema's own statements run in order. A
    /// `test-utils` item, so it is the one place a bare library is opened for
    /// every crate's tests.
    pub fn open_bare_library(self, conn: &rusqlite::Connection) {
        register_bare_function(conn, &COVEN_PATH_TEXT);
        for function in self.functions() {
            register_bare_function(conn, function);
        }
        for collation in self.collations() {
            conn.create_collation(collation.name, collation.compare)
                .unwrap_or_else(|error| panic!("{} registers: {error}", collation.name));
        }
        for statement in coven::followed_tables_sql().chain(self.sql()) {
            conn.execute_batch(statement).expect("the bare library's schema applies");
        }
    }
}

/// `coven_path_text` as coven registers it: coven's own function.
#[cfg(any(test, feature = "test-utils"))]
static COVEN_PATH_TEXT: SchemaFunction =
    SchemaFunction { name: "coven_path_text", arguments: 1, version: 0, compute: coven_path_text };

#[cfg(any(test, feature = "test-utils"))]
fn coven_path_text(arguments: &SqlArguments<'_>) -> rusqlite::Result<rusqlite::types::Value> {
    coven::path_text_sql(arguments.get_raw(0))
}

/// `function` on `conn`, a bare connection.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn register_bare_function(conn: &rusqlite::Connection, function: &SchemaFunction) {
    let deterministic = rusqlite::functions::FunctionFlags::SQLITE_UTF8
        | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC
        | rusqlite::functions::FunctionFlags::SQLITE_INNOCUOUS;
    let compute = function.compute;
    conn.create_scalar_function(
        function.name,
        i32::from(function.arguments),
        deterministic,
        move |context| compute(&SqlArguments::Bare(context)),
    )
    .unwrap_or_else(|error| panic!("{} registers: {error}", function.name));
}
