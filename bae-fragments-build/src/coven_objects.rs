//! coven's objects that app SQL may read, as the SQL that makes them and the
//! SQL functions that SQL and the app's calls name.

use sqlive_compose::compiler::Function;
use std::collections::BTreeSet;

/// What coven's readable objects need in a probe.
pub(crate) struct CovenObjects {
    /// The SQL that makes each table, view, index and trigger.
    pub(crate) sql: String,
    /// The SQL functions coven registers on a store's connection, which its
    /// views and the app's SQL call.
    pub(crate) functions: Vec<Function>,
}

/// The functions app SQL calls that coven's schema text doesn't name: they
/// are registered on a store's connection and no view calls them.
const CALLED_BY_APP_SQL: [&str; 3] = ["coven_key", "coven_file_device", "coven_path_text"];

/// coven's readable objects.
pub(crate) fn coven_objects() -> CovenObjects {
    let sql = coven_schema::readable_objects_sql();
    let functions = functions_named_in(&sql)
        .into_iter()
        .chain(CALLED_BY_APP_SQL.iter().map(|name| (*name).to_owned()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|name| Function { name, arguments: -1, deterministic: true })
        .collect();
    CovenObjects { sql, functions }
}

/// The names `sql` calls as functions among coven's own, `coven_` or
/// `_coven_` followed by `(`. A name that is also a view's, as in `CREATE VIEW
/// coven_circles(`, is in the list too, and harmless: a view and a function
/// have separate names in SQLite.
fn functions_named_in(sql: &str) -> BTreeSet<String> {
    let bytes = sql.as_bytes();
    let mut names = BTreeSet::new();
    let mut at = 0;
    while at < bytes.len() {
        if !is_name_byte(bytes[at]) || (at > 0 && is_name_byte(bytes[at - 1])) {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && is_name_byte(bytes[at]) {
            at += 1;
        }
        let name = &sql[start..at];
        if (name.starts_with("coven_") || name.starts_with("_coven_"))
            && bytes.get(at) == Some(&b'(')
        {
            names.insert(name.to_owned());
        }
    }
    names
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
#[path = "coven_objects_tests.rs"]
mod tests;
