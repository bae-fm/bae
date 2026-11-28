//! Generated statements run on a bare library connection, which a domain's
//! test opens with `Schema::open_bare_library` to write rows past coven: a
//! row another device's write could store, or one a CHECK refuses.

/// Run the generated write `statement` on `conn`, and return how many rows
/// it changed.
pub fn run<Q: coven::Write>(conn: &rusqlite::Connection, statement: &Q) -> rusqlite::Result<usize> {
    conn.execute(<Q as coven::Statement>::SQL, statement.params().as_slice())
}

/// Every row the generated read `statement` returns on `conn`.
pub fn rows<Q: coven::Read>(
    conn: &rusqlite::Connection,
    statement: &Q,
) -> rusqlite::Result<Vec<Q::Row>> {
    let mut prepared = conn.prepare(<Q as coven::Statement>::SQL)?;
    let rows = prepared.query_map(statement.params().as_slice(), Q::read)?;
    rows.collect()
}

/// The one row the generated read `statement` returns on `conn`.
pub fn row<Q: coven::Read>(conn: &rusqlite::Connection, statement: &Q) -> rusqlite::Result<Q::Row> {
    conn.query_row(<Q as coven::Statement>::SQL, statement.params().as_slice(), Q::read)
}
