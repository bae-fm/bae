/// `.cargo/config.toml` builds SQLite without its process-wide memory
/// statistics, which would serialize every allocation through one mutex.
#[test]
fn sqlite_is_built_without_memory_statistics() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let built_without: bool = connection
        .query_row("SELECT sqlite_compileoption_used('DEFAULT_MEMSTATUS=0')", [], |row| row.get(0))
        .unwrap();
    assert!(built_without, "SQLite was built with memory statistics on");
}

/// `.cargo/config.toml` keeps a statement's journal in memory up to
/// 124 KiB, so the FTS5 segment merges bae's triggers run inside a write
/// don't send every later statement's journal to a file.
#[test]
fn sqlite_keeps_a_statement_journal_in_memory_up_to_124_kib() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let spill: bool = connection
        .query_row("SELECT sqlite_compileoption_used('STMTJRNL_SPILL=126976')", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(spill, "SQLite was built with another statement journal spill");
}
