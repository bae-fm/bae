//! The library's schema, which a library's store is created, opened and
//! restored on: each crate's parts of it, in crate order, as `schema.toml`
//! lists them.

include!(concat!(env!("OUT_DIR"), "/schema.rs"));

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
