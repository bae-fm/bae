use super::*;

fn cat(toml: &str) -> Catalog {
    Catalog::from_toml(toml).expect("catalog parses")
}

#[test]
fn apple_plain_and_multiarg() {
    let c = cat(r#"
[messages."core.error.not_found.release"]
value = "that release couldn't be found"
translations = { ar = "تعذر العثور على ذلك الإصدار", "zh-Hans" = "找不到该发行版" }

[messages."core.identify.barcode.looking_up"]
args = { position = "Int", total = "Int" }
value = "Looking up barcode {position} of {total}"
"#);
    let json = apple_xcstrings(&c).unwrap();
    // Multi-arg becomes positional; single message text passes through.
    assert!(json.contains("Looking up barcode %1$lld of %2$lld"), "{json}");
    assert!(json.contains("that release couldn't be found"), "{json}");
    assert!(json.contains("\"sourceLanguage\": \"en\""));
    // Every catalog target locale gets a slot; messages without that
    // locale are `new`.
    assert!(json.contains("\"ar\"") && json.contains("\"zh-Hans\""), "{json}");
    assert!(json.contains("\"new\""), "{json}");
}

#[test]
fn apple_plural_becomes_variations() {
    let c = cat(r#"
[messages."core.outbox.pending_deletes"]
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"
"#);
    let json = apple_xcstrings(&c).unwrap();
    assert!(json.contains("\"plural\""), "{json}");
    assert!(json.contains("%lld pending delete"), "{json}");
    assert!(json.contains("%lld pending deletes"), "{json}");
}

#[test]
fn apple_plural_beside_an_argument_is_a_substitution() {
    let c = cat(r#"
[messages."core.activity.releases_done"]
args = { done = "Int", total = "Int" }
value = "{done} of {total, plural, one {# release} other {# releases}}"
translations = { pl = "{done} z {total, plural, one {# wydania} few {# wydań} many {# wydań} other {# wydania}}" }
"#);
    let json: serde_json::Value = serde_json::from_str(&apple_xcstrings(&c).unwrap()).unwrap();
    let locs = &json["strings"]["core.activity.releases_done"]["localizations"];

    let en = &locs["en"];
    assert_eq!(en["stringUnit"]["value"], "%1$lld of %#@total@");
    let total = &en["substitutions"]["total"];
    assert_eq!(total["argNum"], 2);
    assert_eq!(total["formatSpecifier"], "lld");
    let cases = total["variations"]["plural"].as_object().unwrap();
    assert_eq!(cases.keys().collect::<Vec<_>>(), ["one", "other"]);
    assert_eq!(cases["one"]["stringUnit"]["value"], "%arg release");
    assert_eq!(cases["other"]["stringUnit"]["value"], "%arg releases");

    let pl = &locs["pl"];
    assert_eq!(pl["stringUnit"]["value"], "%1$lld z %#@total@");
    let pl_cases = pl["substitutions"]["total"]["variations"]["plural"].as_object().unwrap();
    let mut keys: Vec<_> = pl_cases.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["few", "many", "one", "other"]);
    assert_eq!(pl["substitutions"]["total"]["argNum"], 2);
}

#[test]
fn apple_plural_with_another_argument_in_a_case_is_a_substitution() {
    let c = cat(r#"
[messages."core.bad"]
args = { count = "Int", name = "Str" }
value = "{count, plural, one {# thing for {name}} other {# things for {name}}}"
"#);
    let json: serde_json::Value = serde_json::from_str(&apple_xcstrings(&c).unwrap()).unwrap();
    let en = &json["strings"]["core.bad"]["localizations"]["en"];
    assert_eq!(en["stringUnit"]["value"], "%#@count@");
    assert_eq!(
        en["substitutions"]["count"]["variations"]["plural"]["one"]["stringUnit"]["value"],
        "%arg thing for %2$@"
    );
}

#[test]
fn apple_rejects_a_plural_nested_in_a_plural() {
    let c = cat(r#"
[messages."core.bad"]
args = { a = "Int", b = "Int" }
value = "{a, plural, one {{b, plural, one {x} other {y}}} other {z}}"
"#);
    assert!(apple_xcstrings(&c).is_err());
}

#[test]
fn apple_rejects_two_plurals_in_one_message() {
    let c = cat(r#"
[messages."core.bad"]
args = { a = "Int", b = "Int" }
value = "{a, plural, one {# x} other {# xs}} {b, plural, one {# y} other {# ys}}"
"#);
    assert!(apple_xcstrings(&c).is_err());
}

#[test]
fn android_sanitizes_id_and_keeps_mf_verbatim() {
    let c = cat(r#"
[messages."core.outbox.pending_deletes"]
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"

[messages."core.error.not_found.release"]
value = "that release couldn't be found"
"#);
    let xml = android_strings_xml(&c, "en");
    assert!(xml.contains("name=\"core_outbox_pending_deletes\""), "{xml}");
    // MF1 braces kept verbatim for android.icu.text.MessageFormat.
    assert!(xml.contains("{count, plural, one {# pending delete}"), "{xml}");
    // Apostrophe escaped for the resource parser.
    assert!(xml.contains("couldn\\'t"), "{xml}");
}

#[test]
fn android_preserves_boundary_whitespace() {
    let c = cat(r#"
[messages."core.audio.list_separator"]
value = " · "
"#);
    let xml = android_strings_xml(&c, "en");
    assert!(xml.contains("<string name=\"core_audio_list_separator\">\" · \"</string>"), "{xml}");
}

#[test]
fn android_id_sanitization() {
    assert_eq!(sanitize_android_id("core.a.b-c"), "core_a_b_c");
}

#[test]
fn android_values_dir_qualifiers() {
    // Bare language: plain qualifier.
    assert_eq!(android_values_dir("es"), "values-es");
    // Region / script subtags need the BCP-47 `b+` form.
    assert_eq!(android_values_dir("pt-BR"), "values-b+pt+BR");
    assert_eq!(android_values_dir("zh-Hans"), "values-b+zh+Hans");
}

#[test]
fn android_resource_files_cover_source_and_target_locales() {
    let c = cat(r#"
[messages."core.audio.channels.mono"]
value = "mono"
translations = { ar = "أحادي", "zh-Hans" = "单声道" }
"#);
    let files = android_resource_files(&c);
    // One source `values/` file plus one per shipping locale, translated or
    // not.
    assert_eq!(files.len(), 1 + TARGET_LOCALES.len());
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert!(paths.contains(&"values/core_strings.xml"), "{paths:?}");
    assert!(paths.contains(&"values-b+zh+Hans/core_strings.xml"), "{paths:?}");
    assert!(paths.contains(&"values-ar/core_strings.xml"), "{paths:?}");
}

/// A translation that puts the arguments in another order keeps each
/// argument's source position, which is the order a caller passes them.
#[test]
fn apple_positions_follow_the_source_order() {
    let c = cat(r#"
[messages."core.pressing.media_count"]
args = { count = "Int", medium = "Str" }
value = "{count}×{medium}"
translations = { ja = "{medium}×{count}" }
"#);
    let json = apple_xcstrings(&c).unwrap();
    assert!(json.contains("%1$lld×%2$@"), "{json}");
    assert!(json.contains("%2$@×%1$lld"), "{json}");
}

#[test]
fn translations_emit_per_locale_with_locale_plural_categories() {
    // A plain message translated into one locale, and a plural translated
    // into a locale with more CLDR categories than English (Polish:
    // one/few/many/other).
    let c = cat(r#"
[messages."core.error.not_found.release"]
value = "that release couldn't be found"
translations = { es = "no se encontró ese lanzamiento" }

[messages."core.outbox.pending_deletes"]
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"
translations = { pl = "{count, plural, one {# usunięcie oczekuje} few {# usunięcia oczekują} many {# usunięć oczekuje} other {# usunięcia oczekuje}}" }
"#);

    // Apple: the Spanish slot carries the translation; the Polish plural
    // expands to all four of its categories.
    let json = apple_xcstrings(&c).unwrap();
    assert!(json.contains("no se encontró ese lanzamiento"), "{json}");
    assert!(json.contains("%lld usunięcia oczekują"), "{json}");
    assert!(json.contains("%lld usunięć oczekuje"), "{json}");

    // Android: the Spanish file carries the translation verbatim; a target
    // locale untranslated for this message keeps the English source.
    let files = android_resource_files(&c);
    let es = files.iter().find(|(p, _)| p == "values-es/core_strings.xml").unwrap();
    assert!(es.1.contains("no se encontró ese lanzamiento"), "{}", es.1);
    let pl = files.iter().find(|(p, _)| p == "values-pl/core_strings.xml").unwrap();
    assert!(pl.1.contains("that release couldn"), "{}", pl.1);
}
