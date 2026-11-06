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
