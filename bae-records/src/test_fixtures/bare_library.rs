//! An in-memory library on the records' test schema, outside coven, on a
//! connection a records test opens itself: for tests of SQL that runs on a
//! bare connection.

/// Open the records' test schema on `conn`, a fresh in-memory connection.
pub(crate) fn open_bare_library(conn: &rusqlite::Connection) {
    let parts = super::TEST_SCHEMA.bare_library_parts();
    let deterministic = rusqlite::functions::FunctionFlags::SQLITE_UTF8
        | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC
        | rusqlite::functions::FunctionFlags::SQLITE_INNOCUOUS;
    for function in parts.functions {
        let compute = function.compute;
        conn.create_scalar_function(
            function.name,
            i32::from(function.arguments),
            deterministic,
            move |context| compute(&bae_store::schema::SqlArguments::Bare(context)),
        )
        .unwrap_or_else(|error| panic!("{} registers: {error}", function.name));
    }
    for collation in parts.collations {
        conn.create_collation(collation.name, collation.compare)
            .unwrap_or_else(|error| panic!("{} registers: {error}", collation.name));
    }
    for statement in parts.statements {
        conn.execute_batch(statement).expect("the bare library's schema applies");
    }
}
