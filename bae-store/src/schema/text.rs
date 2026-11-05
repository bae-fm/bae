//! The text functions and the collation the schema names: library search's
//! folding and pattern, and the order a person reads names in.

use rusqlite::types::Value;

use super::{SchemaCollation, SchemaFunction};

/// `bae_fold_case`: a text folded as library search folds it; NULL stays NULL.
pub static FOLD_CASE: SchemaFunction = SchemaFunction {
    name: "bae_fold_case",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        Ok(match arguments.get::<Option<String>>(0)? {
            Some(text) => Value::Text(bae_util::search_text::fold_case(&text)),
            None => Value::Null,
        })
    },
};

/// `bae_search_pattern`: the `GLOB` pattern library search matches a query
/// by; NULL stays NULL.
pub static SEARCH_PATTERN: SchemaFunction = SchemaFunction {
    name: "bae_search_pattern",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        Ok(match arguments.get::<Option<String>>(0)? {
            Some(query) => Value::Text(bae_util::search_text::search_pattern(&query)),
            None => Value::Null,
        })
    },
};

/// The collation the schema names for folder paths in the order a person
/// reads them. A change to what it compares bumps its version, and coven
/// rebuilds the indexes that use it on the next writable open.
pub static NATURAL_ORDER: SchemaCollation = SchemaCollation {
    name: "natural_order",
    version: 1,
    compare: bae_util::natural_order::compare,
};
