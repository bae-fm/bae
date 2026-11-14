//! Damage planted in a library's file for a test of what reads and writes do
//! over it: a row no write stores, a trigger that fails a write, a table
//! gone. Only a domain's named damage functions call this, each with the
//! statements that name its damage, as a damaged or full disk would leave
//! its tables.

/// Run `damage` on the library file under `directory` that holds `table`,
/// from a connection of its own, past the store and its checks. Panics when
/// no library file there holds the table.
pub fn plant_in_library_file(directory: &std::path::Path, table: &str, damage: &str) {
    let mut planted = false;
    let mut dirs = vec![directory.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("the library's directory reads") {
            let path = entry.expect("the library's directory lists").path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let is_database =
                std::fs::read(&path).is_ok_and(|bytes| bytes.starts_with(b"SQLite format 3\0"));
            if !is_database {
                continue;
            }
            let connection = rusqlite::Connection::open(&path).expect("the library's file opens");
            // The functions the schema's triggers call, which a write the
            // damage makes runs.
            for function in [&crate::FOLD_CASE, &crate::NAME_KEY, &crate::NAME_LIST_KEY] {
                crate::schema::register_bare_function(&connection, function);
            }
            let holds_table: bool = connection
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = ?)",
                    [table],
                    |row| row.get(0),
                )
                .expect("the library's schema reads");
            if !holds_table {
                continue;
            }
            connection.execute_batch(damage).expect("the damage is planted");
            planted = true;
        }
    }
    assert!(planted, "the library under the directory holds {table}");
}

/// Take out every trigger `sql_text` (a domain's SQL file) creates, from the
/// library file under `directory` that holds `table`: a family of triggers
/// removed from a closed copy, to time a write without it. An INSTEAD OF
/// trigger, which other triggers call through its view, is replaced by one
/// that does nothing, so those callers still run.
pub fn drop_triggers_of(directory: &std::path::Path, table: &str, sql_text: &str) {
    const CREATE: &str = "CREATE TRIGGER IF NOT EXISTS ";
    const INSTEAD_OF: &str = " INSTEAD OF INSERT ON ";
    let damage: String = sql_text
        .match_indices(CREATE)
        .map(|(at, _)| {
            let rest = &sql_text[at + CREATE.len()..];
            let name: String =
                rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let header = &rest[..rest.find("BEGIN").expect("a trigger has a body")];
            let mut statement = format!("DROP TRIGGER IF EXISTS {name};");
            if let Some(view) = header.split(INSTEAD_OF).nth(1) {
                let view = view.trim();
                statement.push_str(&format!(
                    "CREATE TRIGGER {name} INSTEAD OF INSERT ON {view} BEGIN SELECT 1; END;"
                ));
            }
            statement
        })
        .collect();
    assert!(!damage.is_empty(), "{table}'s SQL creates no trigger");
    plant_in_library_file(directory, table, &damage);
}
