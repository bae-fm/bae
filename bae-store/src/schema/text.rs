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

/// `bae_name_key(name, language)`: the key a name is ordered by in a language
/// (`bae_util::name_key`); NULL stays NULL. `language` is the tag the
/// device's `name_sort` row holds, which is a language tag.
pub static NAME_KEY: SchemaFunction = SchemaFunction {
    name: "bae_name_key",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let Some(name) = arguments.get::<Option<String>>(0)? else {
            return Ok(Value::Null);
        };
        let keys = name_keys(&arguments.get::<String>(1)?)?;
        Ok(Value::Blob(keys.key(&name)))
    },
};

/// `bae_name_list_key(names, language)`: the key a list of names, joined by
/// `char(31)`, is ordered by in a language (`bae_util::name_key`); NULL stays
/// NULL.
pub static NAME_LIST_KEY: SchemaFunction = SchemaFunction {
    name: "bae_name_list_key",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let Some(names) = arguments.get::<Option<String>>(0)? else {
            return Ok(Value::Null);
        };
        let keys = name_keys(&arguments.get::<String>(1)?)?;
        Ok(Value::Blob(keys.list_key(&names)))
    },
};

fn name_keys(language: &str) -> rusqlite::Result<std::sync::Arc<bae_util::name_key::NameKeys>> {
    bae_util::name_key::NameKeys::for_language(language)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
}

/// The collation the schema names for folder paths in the order a person
/// reads them. A change to what it compares bumps its version, and coven
/// rebuilds the indexes that use it on the next writable open.
pub static NATURAL_ORDER: SchemaCollation = SchemaCollation {
    name: "natural_order",
    version: 1,
    compare: bae_util::natural_order::compare,
};
