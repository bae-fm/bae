//! The SQL function that trims a title the way the code that writes it
//! does, so the import pane's count of untitled tracks and the writes share
//! one definition of an empty title.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_trim(text)`: the text without the whitespace `str::trim` removes.
pub(crate) static TRIM: SchemaFunction = SchemaFunction {
    name: "bae_trim",
    arguments: 1,
    version: 1,
    compute: |arguments| Ok(Value::Text(arguments.get::<String>(0)?.trim().to_owned())),
};
