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

#[test]
fn flags_a_translation_that_ends_a_sentence_twice() {
    let c = cat(r#"
[messages."core.x"]
value = "Reconnect to play this release. It isn’t downloaded yet."
translations = { zh-Hans = "请重新连接以播放此发行版。。它尚未下载。" }
"#);
    let errs = validate(&c).unwrap_err();
    assert_eq!(errs, ["core.x [zh-Hans]: `。。` ends one sentence twice"]);
}

#[test]
fn flags_three_full_stops_written_for_an_ellipsis() {
    let c = cat(r#"
[messages."core.x"]
value = "Checking the bucket..."
translations = { de = "Bucket wird geprüft..." }
"#);
    let errs = validate(&c).unwrap_err();
    assert_eq!(
        errs,
        [
            "core.x [en]: `...` is an ellipsis written as three full stops; write `…`",
            "core.x [de]: `...` is an ellipsis written as three full stops; write `…`",
        ]
    );
}

#[test]
fn flags_a_straight_apostrophe() {
    let c = cat(r#"
[messages."core.x"]
value = "It isn't downloaded"
translations = { fr = "Il n'est pas téléchargé" }
"#);
    let errs = validate(&c).unwrap_err();
    assert_eq!(
        errs,
        [
            "core.x [en]: `'` is an apostrophe written as a straight quote; write `’`",
            "core.x [fr]: `'` is an apostrophe written as a straight quote; write `’`",
        ]
    );
}

#[test]
fn accepts_the_curly_apostrophe() {
    let c = cat(r#"
[messages."core.x"]
value = "It isn’t downloaded"
translations = { fr = "Il n’est pas téléchargé" }
"#);
    assert!(validate(&c).is_ok());
}

#[test]
fn accepts_the_ellipsis_character() {
    let c = cat(r#"
[messages."core.x"]
value = "Checking the bucket…"
translations = { de = "Bucket wird geprüft…" }
"#);
    assert!(validate(&c).is_ok());
}

#[test]
fn flags_an_english_source_that_breaks_a_sentence_with_a_dash() {
    for value in ["No matches — try different terms", "No matches – try different terms"] {
        let c = cat(&format!("[messages.\"core.x\"]\nvalue = \"{value}\"\n"));
        let errs = validate(&c).unwrap_err();
        assert_eq!(errs, ["core.x: the English source breaks a sentence with a dash"]);
    }
}

fn one_message(translations: &str) -> Catalog {
    cat(&format!("[messages.\"core.x\"]\nvalue = \"hi\"\ntranslations = {{ {translations} }}\n"))
}

#[test]
fn flags_a_locale_outside_the_shipping_set() {
    let mut translations: Vec<String> =
        TARGET_LOCALES.iter().map(|l| format!("\"{l}\" = \"hi\"")).collect();
    translations.push("\"qq\" = \"hi\"".to_string());
    let errs = locale_coverage(&one_message(&translations.join(", "))).unwrap_err();
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("`qq` is not a shipping locale"), "{errs:?}");
}

#[test]
fn flags_a_shipping_locale_with_no_translation() {
    let errs = locale_coverage(&one_message("de = \"hallo\"")).unwrap_err();
    assert_eq!(errs.len(), TARGET_LOCALES.len() - 1, "{errs:?}");
    assert!(errs.iter().any(|e| e.contains("`fr` ships, but 1 message")), "{errs:?}");
}

#[test]
fn accepts_exactly_the_shipping_set() {
    let translations: Vec<String> =
        TARGET_LOCALES.iter().map(|l| format!("\"{l}\" = \"hi\"")).collect();
    assert!(locale_coverage(&one_message(&translations.join(", "))).is_ok());
}

fn real_catalog() -> Catalog {
    let src = include_str!("../../bae-bridge/loc/catalog.toml");
    Catalog::from_toml(src).expect("real catalog parses")
}

/// The shipping catalog must always be valid. This gates `cargo test`, so an
/// invalid edit to `bae-bridge/loc/catalog.toml` fails the build.
#[test]
fn real_catalog_is_valid() {
    if let Err(errs) = validate(&real_catalog()) {
        panic!("real catalog invalid:\n  - {}", errs.join("\n  - "));
    }
}

/// The shipping catalog translates every locale bae ships and nothing else.
/// A gap here means the app declares a locale it would serve English under.
#[test]
fn real_catalog_translates_exactly_the_shipping_locales() {
    if let Err(errs) = locale_coverage(&real_catalog()) {
        panic!("real catalog locale coverage:\n  - {}", errs.join("\n  - "));
    }
}
