//! Catalog internal-consistency validation, run by `loc-gen check`.
//!
//! This guards the catalog against the mistakes that would otherwise surface as
//! a wrong or missing string at runtime: an undeclared placeholder, a declared
//! argument the message never uses, a plural over a non-integer, a malformed MF1
//! value, or an id outside the `core.`/`ui.` namespaces. A failure here fails
//! the build (the pre-commit hook), so the catalog can't drift into an invalid
//! shape unnoticed.
//!
//! The cross-check between catalog ids and the Rust message enums is the
//! `loc_key_coverage` test module in `bae-bridge/src/types.rs`; this module is
//! the catalog-only half.

use crate::mf1::{self, Node};
use crate::{validate_namespace, ArgType, Catalog};

/// Validate every message. Returns all problems found (not just the first) so a
/// catalog edit gets the full list in one pass.
pub fn validate(cat: &Catalog) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    for (id, msg) in &cat.messages {
        if let Err(e) = validate_namespace(id) {
            errs.push(e);
        }
        let nodes = match mf1::parse(&msg.value) {
            Ok(n) => n,
            Err(e) => {
                errs.push(format!("{id}: {e}"));
                continue;
            }
        };
        let referenced = mf1::referenced_args(&nodes);
        for arg in &referenced {
            if !msg.args.contains_key(arg) {
                errs.push(format!(
                    "{id}: `{{{arg}}}` is used but not declared in `args`"
                ));
            }
        }
        for arg in msg.args.keys() {
            if !referenced.contains(arg) {
                errs.push(format!("{id}: declared argument `{arg}` is never used"));
            }
        }
        for node in &nodes {
            if let Node::Plural { arg, .. } = node {
                if msg.args.get(arg) != Some(&ArgType::Int) {
                    errs.push(format!("{id}: plural argument `{arg}` must be `Int`"));
                }
            }
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(toml: &str) -> Catalog {
        Catalog::from_toml(toml).expect("parses")
    }

    #[test]
    fn accepts_a_well_formed_catalog() {
        let c = cat(r#"
[messages."core.outbox.pending_deletes"]
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"

[messages."core.identify.barcode.looking_up"]
args = { position = "Int", total = "Int" }
value = "Looking up barcode {position} of {total}"

[messages."ui.library.remove_from_device"]
value = "remove this library from this device"
"#);
        assert!(validate(&c).is_ok());
    }

    #[test]
    fn flags_undeclared_placeholder() {
        let c = cat(r#"
[messages."core.x"]
value = "hi {name}"
"#);
        let errs = validate(&c).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("not declared")), "{errs:?}");
    }

    #[test]
    fn flags_unused_declared_arg() {
        let c = cat(r#"
[messages."core.x"]
args = { name = "Str" }
value = "no placeholder here"
"#);
        let errs = validate(&c).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("never used")), "{errs:?}");
    }

    #[test]
    fn flags_non_int_plural_arg() {
        let c = cat(r#"
[messages."core.x"]
args = { count = "Str" }
value = "{count, plural, one {#} other {#}}"
"#);
        let errs = validate(&c).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("must be `Int`")), "{errs:?}");
    }

    #[test]
    fn flags_bad_namespace() {
        let c = cat(r#"
[messages."misc.x"]
value = "oops"
"#);
        assert!(validate(&c).is_err());
    }

    /// The shipping catalog must always be valid. This gates `cargo test`, so an
    /// invalid edit to `bae-bridge/loc/catalog.toml` fails the build.
    #[test]
    fn real_catalog_is_valid() {
        let src = include_str!("../../bae-bridge/loc/catalog.toml");
        let cat = Catalog::from_toml(src).expect("real catalog parses");
        if let Err(errs) = validate(&cat) {
            panic!("real catalog invalid:\n  - {}", errs.join("\n  - "));
        }
    }
}
