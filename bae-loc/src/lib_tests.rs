use super::*;

#[test]
fn parses_args_plural_and_plain() {
    let src = r#"
[messages."core.outbox.pending_deletes"]
comment = "Storage queue: number of cloud deletes still pending."
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"

[messages."core.identify.barcode.looking_up"]
args = { position = "Int", total = "Int" }
value = "Looking up barcode {position} of {total}"

[messages."ui.library.remove_from_device"]
value = "remove this library from this device"
"#;
    let cat = Catalog::from_toml(src).expect("catalog parses");
    assert_eq!(cat.messages.len(), 3);

    let deletes = &cat.messages["core.outbox.pending_deletes"];
    assert_eq!(deletes.args["count"], ArgType::Int);
    assert!(deletes.value.contains("plural"));

    let barcode = &cat.messages["core.identify.barcode.looking_up"];
    assert_eq!(barcode.args.len(), 2);
    assert_eq!(barcode.args["total"], ArgType::Int);

    let chrome = &cat.messages["ui.library.remove_from_device"];
    assert!(chrome.args.is_empty());
    assert_eq!(chrome.comment, None);
}

#[test]
fn validates_namespaces() {
    assert!(validate_namespace("core.error.x").is_ok());
    assert!(validate_namespace("ui.button.ok").is_ok());
    assert!(validate_namespace("misc.oops").is_err());
}

#[test]
fn the_source_locale_is_not_a_target() {
    assert!(!TARGET_LOCALES.contains(&SOURCE_LOCALE));
}

#[test]
fn every_target_locale_is_named_once() {
    let mut seen = std::collections::BTreeSet::new();
    for locale in TARGET_LOCALES {
        assert!(seen.insert(*locale), "`{locale}` is listed twice");
    }
}
