//! What a schema's tests check of it whatever parts it is made of: each check
//! names what breaks the rule, which the test asserts is nothing.

use crate::Schema;

/// The tables `schema` creates that aren't `STRICT`, which store a value of
/// the wrong type rather than refuse it.
pub fn loose_tables(schema: Schema) -> Vec<String> {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    schema.open_bare_library(&conn);
    let loose = conn
        .prepare(
            "SELECT name FROM pragma_table_list
             WHERE schema = 'main' AND type = 'table' AND NOT strict
               AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    loose
}

/// The tables of bae's own that `schema` creates with no primary key, which
/// coven can't tell the rows of apart: a read of such a table reruns for every
/// write to it. coven's own `coven_` tables are coven's to key.
pub fn keyless_tables(schema: Schema) -> Vec<String> {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    schema.open_bare_library(&conn);
    let keyless = conn
        .prepare(
            "SELECT name FROM pragma_table_list AS t
             WHERE schema = 'main' AND type = 'table'
               AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
               AND name NOT LIKE 'coven\\_%' ESCAPE '\\'
               AND NOT EXISTS (SELECT 1 FROM pragma_table_info(t.name) WHERE pk > 0)
             ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    keyless
}

/// The names of the tables and triggers `schema` creates.
pub fn tables_and_triggers(schema: Schema) -> Vec<String> {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    schema.open_bare_library(&conn);
    let names = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type IN ('table', 'trigger') ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    names
}

/// The tables `schema` creates that name `table` in a foreign key, by name.
pub fn tables_referencing(schema: Schema, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    schema.open_bare_library(&conn);
    let referencing = conn
        .prepare(
            "SELECT DISTINCT m.name FROM sqlite_schema AS m, pragma_foreign_key_list(m.name) AS f
             WHERE m.type = 'table' AND f.\"table\" = ?
             ORDER BY m.name",
        )
        .unwrap()
        .query_map([table], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    referencing
}

/// Each column an `UPDATE OF` trigger of `schema` names that its table
/// doesn't have, as `trigger: table.column`. SQLite accepts a name no column
/// has, and the trigger then never fires on it.
pub fn absent_update_of_columns(schema: Schema) -> Vec<String> {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    schema.open_bare_library(&conn);
    let update_of = regex::Regex::new(r"(?is)UPDATE OF\s+(.+?)\s+ON\s+(\w+)").unwrap();
    let triggers: Vec<(String, String)> = conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE type = 'trigger'")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut checked = 0;
    let mut missing = Vec::new();
    for (trigger, sql) in &triggers {
        let Some(captures) = update_of.captures(sql) else {
            continue;
        };
        checked += 1;
        let table = &captures[2];
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_xinfo(?)")
            .unwrap()
            .query_map([table], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for named in captures[1].split(',').map(str::trim) {
            if !columns.iter().any(|column| column == named) {
                missing.push(format!("{trigger}: {table}.{named}"));
            }
        }
    }
    assert!(checked > 0, "no UPDATE OF trigger found");
    missing
}

/// The names of the triggers `schema` creates on `table`.
pub fn triggers_on(schema: Schema, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    schema.open_bare_library(&conn);
    let triggers = conn
        .prepare(
            "SELECT name FROM sqlite_schema WHERE type = 'trigger' AND tbl_name = ? ORDER BY name",
        )
        .unwrap()
        .query_map([table], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    triggers
}

/// The `CREATE TABLE` statement `schema` creates `table` with.
pub fn table_definition(schema: Schema, table: &str) -> String {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    schema.open_bare_library(&conn);
    conn.query_row(
        "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?",
        [table],
        |row| row.get(0),
    )
    .unwrap_or_else(|error| panic!("the schema creates no table {table}: {error}"))
}
