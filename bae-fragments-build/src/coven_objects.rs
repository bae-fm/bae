//! coven's objects that app SQL may read, as the SQL that makes them and the
//! SQL functions that SQL calls.

use sqlive_compose::compiler::Function;

/// What coven's readable objects need in a probe.
pub(crate) struct CovenObjects {
    /// The SQL that makes each table, view, index and trigger.
    pub(crate) sql: String,
    /// The SQL functions coven registers on a store's connection, which its
    /// views and the app's SQL call.
    pub(crate) functions: Vec<Function>,
}

/// coven's readable objects.
pub(crate) fn coven_objects() -> CovenObjects {
    CovenObjects {
        sql: coven_schema::readable_objects_sql(),
        functions: coven_schema::SQL_FUNCTIONS
            .iter()
            .map(|function| Function {
                name: function.name.to_owned(),
                arguments: function.arguments,
                deterministic: function.deterministic,
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "coven_objects_tests.rs"]
mod tests;
