//! Convert a parsed catalog into each platform's native resource file.
//!
//! Apple is the only target that needs structural conversion: ICU MessageFormat
//! named args become positional `%lld`/`%@`, and a whole-message `plural`
//! becomes a String Catalog `variations.plural`. Android and Windows store the
//! MF1 string **verbatim** (their runtimes parse it) — the only work there is
//! resource-file escaping and, for Android, sanitizing the dotted id into a
//! legal resource name.
//!
//! Only the message shapes bae actually authors are handled; anything else
//! (plural combined with extra args, a plural embedded mid-sentence) returns an
//! error so the build fails loudly instead of emitting wrong output.

use crate::mf1::{self, Node, PluralSelector};
use crate::{ArgType, Catalog, Message};

use crate::mf1::referenced_args as ordered_args;

fn apple_specifier(ty: ArgType) -> &'static str {
    match ty {
        ArgType::Int => "lld",
        ArgType::Str => "@",
    }
}

/// Render a flat (non-plural) node run into an Apple format string. `count_arg`,
/// when set, is the plural argument whose `#`/name both render as that single
/// argument (always positional 1 inside a plural body).
fn apple_flat(
    nodes: &[Node],
    msg: &Message,
    order: &[String],
    count_arg: Option<&str>,
) -> Result<String, String> {
    let mut out = String::new();
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(&t.replace('%', "%%")),
            Node::Pound => {
                let arg = count_arg.ok_or("`#` used outside a plural")?;
                let ty =
                    msg.args.get(arg).copied().ok_or_else(|| {
                        format!("plural argument `{arg}` is not declared in `args`")
                    })?;
                out.push_str(&format!("%{}", apple_specifier(ty)));
            }
            Node::Arg(name) => {
                let ty = msg
                    .args
                    .get(name)
                    .copied()
                    .ok_or_else(|| format!("argument `{name}` is not declared in `args`"))?;
                // The plural count and any single-arg message render
                // non-positional (`%lld`); only genuinely multi-arg messages
                // need positional specifiers.
                if Some(name.as_str()) == count_arg || order.len() <= 1 {
                    out.push_str(&format!("%{}", apple_specifier(ty)));
                } else {
                    let pos = order.iter().position(|a| a == name).unwrap() + 1;
                    out.push_str(&format!("%{}${}", pos, apple_specifier(ty)));
                }
            }
            Node::Plural { .. } => {
                return Err("nested or embedded plural is not supported".to_string())
            }
        }
    }
    Ok(out)
}

fn json_str(s: &str) -> serde_json::Value {
    serde_json::Value::String(s.to_string())
}

fn string_unit_state(value: &str, state: &str) -> serde_json::Value {
    serde_json::json!({ "stringUnit": { "state": state, "value": value } })
}

/// The MF1 value and translation state to emit for `msg` in `locale`. The source
/// language, and any locale with an explicit `translations` entry, are
/// `translated`; a locale with no entry falls back to the English source at
/// state `new` (the app still declares the locale; English shows until the slot
/// is filled in).
fn localized<'a>(msg: &'a Message, locale: &str, src_lang: &str) -> (&'a str, &'static str) {
    if locale == src_lang {
        (msg.value.as_str(), "translated")
    } else if let Some(t) = msg.translations.get(locale) {
        (t.as_str(), "translated")
    } else {
        (msg.value.as_str(), "new")
    }
}

/// Render one locale's MF1 `value` for `msg` into an `.xcstrings` unit: a
/// `variations.plural` for a whole-message plural, otherwise a flat
/// `stringUnit`. Each locale's value is parsed independently, so a translation
/// may carry that locale's own plural categories (one/few/many/other) where the
/// English source has only one/other.
fn apple_unit(msg: &Message, value: &str, state: &str) -> Result<serde_json::Value, String> {
    let nodes = mf1::parse(value)?;
    let order = ordered_args(&nodes);

    // Whole-message plural -> variations.plural. Other plural shapes are errors.
    if let [Node::Plural { arg, cases }] = nodes.as_slice() {
        if msg.args.len() != 1 || !msg.args.contains_key(arg) {
            return Err(format!(
                "plural message `{value}` must take exactly its count arg `{arg}`"
            ));
        }
        let mut variations = serde_json::Map::new();
        for case in cases {
            let cat = match &case.selector {
                PluralSelector::Category(c) => c.as_cldr(),
                PluralSelector::Exact(_) => {
                    return Err("`=N` exact plural cases are not supported on Apple yet".to_string())
                }
            };
            let rendered = apple_flat(&case.message, msg, &order, Some(arg))?;
            variations.insert(cat.to_string(), string_unit_state(&rendered, state));
        }
        return Ok(serde_json::json!({ "variations": { "plural": variations } }));
    }

    if nodes.iter().any(|n| matches!(n, Node::Plural { .. })) {
        return Err(format!(
            "message `{value}` embeds a plural mid-text; Apple needs a substitution, not yet supported"
        ));
    }

    let rendered = apple_flat(&nodes, msg, &order, None)?;
    Ok(string_unit_state(&rendered, state))
}

/// Build the `.xcstrings` `localizations` body for one message: the source
/// locale (state `translated`) plus every catalog target locale — its
/// translation at state `translated`, or the source value at `new` where that
/// locale isn't translated yet.
fn apple_localization(
    msg: &Message,
    src_lang: &str,
    target_locales: &[&str],
) -> Result<serde_json::Value, String> {
    let mut locs = serde_json::Map::new();
    locs.insert(
        src_lang.to_string(),
        apple_unit(msg, &msg.value, "translated")?,
    );
    for loc in target_locales {
        let (value, state) = localized(msg, loc, src_lang);
        locs.insert((*loc).to_string(), apple_unit(msg, value, state)?);
    }
    Ok(serde_json::Value::Object(locs))
}

/// Emit the source-language `Core.xcstrings` for the whole catalog.
pub fn apple_xcstrings(cat: &Catalog, src_lang: &str) -> Result<String, String> {
    let mut strings = serde_json::Map::new();
    let target_locales = cat.target_locales(src_lang);
    for (id, msg) in &cat.messages {
        let mut entry = serde_json::Map::new();
        if let Some(comment) = &msg.comment {
            entry.insert("comment".to_string(), json_str(comment));
        }
        entry.insert(
            "localizations".to_string(),
            apple_localization(msg, src_lang, &target_locales)?,
        );
        strings.insert(id.clone(), serde_json::Value::Object(entry));
    }
    let doc = serde_json::json!({
        "sourceLanguage": src_lang,
        "strings": serde_json::Value::Object(strings),
        "version": "1.0",
    });
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

/// Android resource names allow `[a-z0-9_]`; the dotted id maps by replacing
/// `.` and `-` with `_`. The MF1 *value* is untouched.
pub fn sanitize_android_id(id: &str) -> String {
    id.replace(['.', '-'], "_")
}

/// Escape a string for storage inside an Android `<string>` resource so that
/// `getString` returns it byte-for-byte (then handed to MessageFormat). Android
/// resources are XML, so this is `xml_escape` plus the backslash-escaping the
/// Android resource parser additionally requires for quotes.
fn android_escape(s: &str) -> String {
    let mut out = String::new();
    for c in xml_escape(s).chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out
}

/// Emit `core_strings.xml` for one `locale`. Every message is a plain
/// `<string>`; the MF1 value for that locale — its translation, or the English
/// source where untranslated — is stored verbatim (escaped) for
/// `android.icu.text.MessageFormat`.
pub fn android_strings_xml(cat: &Catalog, locale: &str, src_lang: &str) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n");
    for (id, msg) in &cat.messages {
        let (value, _state) = localized(msg, locale, src_lang);
        out.push_str(&format!(
            "    <string name=\"{}\">{}</string>\n",
            sanitize_android_id(id),
            android_escape(value),
        ));
    }
    out.push_str("</resources>\n");
    out
}

/// Map a catalog locale code to the Android resource-qualifier directory that
/// carries it. A bare language is `values-<lang>`; a code with a region or
/// script subtag needs the BCP-47 `b+` form (`values-b+pt+BR`,
/// `values-b+zh+Hans`) — the legacy `values-pt-rBR` form can't express a script
/// like `Hans`, so use `b+` uniformly for any multi-subtag code.
fn android_values_dir(locale: &str) -> String {
    if let Some((lang, rest)) = locale.split_once('-') {
        format!("values-b+{lang}+{rest}")
    } else {
        format!("values-{locale}")
    }
}

/// Emit the full Android resource set: the source language under `values/`, plus
/// one `values-<qualifier>/core_strings.xml` per catalog target locale carrying
/// that locale's translations (source-language values where a message isn't
/// translated yet).
/// Android resolves resources per directory, so a locale the app should
/// "support" must have its own directory — without it the locale falls back to
/// `values/` and never registers as supported (so its plural rules and
/// right-to-left layout selection don't apply). Returns `(relative path, file
/// contents)` pairs.
pub fn android_resource_files(cat: &Catalog, src_lang: &str) -> Vec<(String, String)> {
    let mut files = vec![(
        "values/core_strings.xml".to_string(),
        android_strings_xml(cat, src_lang, src_lang),
    )];
    for loc in cat.target_locales(src_lang) {
        files.push((
            format!("{}/core_strings.xml", android_values_dir(loc)),
            android_strings_xml(cat, loc, src_lang),
        ));
    }
    files
}

fn xml_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Emit `Core.resw` for one `locale`. The dotted id is the resource name; the
/// MF1 value for that locale — its translation, or the English source where
/// untranslated — is stored verbatim (XML-escaped) for the `MessageFormat` NuGet
/// at runtime.
pub fn windows_resw(cat: &Catalog, locale: &str, src_lang: &str) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n  \
         <resheader name=\"resmimetype\"><value>text/microsoft-resx</value></resheader>\n  \
         <resheader name=\"version\"><value>2.0</value></resheader>\n",
    );
    for (id, msg) in &cat.messages {
        let (value, _state) = localized(msg, locale, src_lang);
        out.push_str(&format!(
            "  <data name=\"{}\" xml:space=\"preserve\"><value>{}</value></data>\n",
            xml_escape(id),
            xml_escape(value),
        ));
    }
    out.push_str("</root>\n");
    out
}

/// The generated Windows locales in build/BCP-47 form: the source language plus
/// every catalog target locale. Windows resources are per-language directories
/// (unlike Apple's single multi-locale `.xcstrings`), so the catalog fans out to
/// one `Core.resw` per locale.
fn windows_locales(cat: &Catalog, src_lang: &str) -> Vec<String> {
    let mut locales = vec![format!("{src_lang}-US")];
    locales.extend(cat.target_locales(src_lang).into_iter().map(str::to_string));
    locales
}

/// Emit one `<locale>/Core.resw` per shipping locale: `(relative path, contents)`
/// pairs the caller writes under the project's `Strings` directory. Each locale's
/// file carries that locale's translations, falling back to the English source
/// for any message not yet translated. The per-language directories are what make
/// the app multilingual. Mirrors the Apple emitter, which carries the same
/// per-locale values inside one file.
pub fn windows_resw_all(cat: &Catalog, src_lang: &str) -> Vec<(std::path::PathBuf, String)> {
    let src_dir = format!("{src_lang}-US");
    windows_locales(cat, src_lang)
        .into_iter()
        .map(|dir| {
            // The source directory ("en-US") looks up the source language; every
            // other directory's name is the catalog locale code itself.
            let lookup = if dir == src_dir {
                src_lang
            } else {
                dir.as_str()
            };
            let contents = windows_resw(cat, lookup, src_lang);
            (std::path::PathBuf::from(&dir).join("Core.resw"), contents)
        })
        .collect()
}

/// Emit the .NET satellite-assembly ResX set: `Core.resx` (the source language,
/// the invariant fallback the main assembly embeds) plus one `Core.<culture>.resx`
/// per catalog target locale (each a satellite assembly). The ResX schema is
/// identical to the Windows `.resw` — same writer, MF1 value stored verbatim,
/// dotted id as the resource name — so `windows_resw` produces the body. Only the
/// naming differs: .NET keys the fallback chain off the culture in the filename,
/// not a per-language directory the way MRT does.
pub fn resx_all(cat: &Catalog, src_lang: &str) -> Vec<(std::path::PathBuf, String)> {
    let mut files = vec![(
        std::path::PathBuf::from("Core.resx"),
        windows_resw(cat, src_lang, src_lang),
    )];
    for loc in cat.target_locales(src_lang) {
        files.push((
            std::path::PathBuf::from(format!("Core.{loc}.resx")),
            windows_resw(cat, loc, src_lang),
        ));
    }
    files
}

#[cfg(test)]
mod tests {
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
        let json = apple_xcstrings(&c, "en").unwrap();
        // Multi-arg becomes positional; single message text passes through.
        assert!(
            json.contains("Looking up barcode %1$lld of %2$lld"),
            "{json}"
        );
        assert!(json.contains("that release couldn't be found"), "{json}");
        assert!(json.contains("\"sourceLanguage\": \"en\""));
        // Every catalog target locale gets a slot; messages without that
        // locale are `new`.
        assert!(
            json.contains("\"ar\"") && json.contains("\"zh-Hans\""),
            "{json}"
        );
        assert!(json.contains("\"new\""), "{json}");
    }

    #[test]
    fn apple_plural_becomes_variations() {
        let c = cat(r#"
[messages."core.outbox.pending_deletes"]
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"
"#);
        let json = apple_xcstrings(&c, "en").unwrap();
        assert!(json.contains("\"plural\""), "{json}");
        assert!(json.contains("%lld pending delete"), "{json}");
        assert!(json.contains("%lld pending deletes"), "{json}");
    }

    #[test]
    fn apple_rejects_plural_with_extra_arg() {
        let c = cat(r#"
[messages."core.bad"]
args = { count = "Int", name = "Str" }
value = "{count, plural, one {# thing for {name}} other {# things for {name}}}"
"#);
        assert!(apple_xcstrings(&c, "en").is_err());
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
        let xml = android_strings_xml(&c, "en", "en");
        assert!(
            xml.contains("name=\"core_outbox_pending_deletes\""),
            "{xml}"
        );
        // MF1 braces kept verbatim for android.icu.text.MessageFormat.
        assert!(
            xml.contains("{count, plural, one {# pending delete}"),
            "{xml}"
        );
        // Apostrophe escaped for the resource parser.
        assert!(xml.contains("couldn\\'t"), "{xml}");
    }

    #[test]
    fn windows_keeps_dotted_id_and_mf_verbatim() {
        let c = cat(r#"
[messages."core.identify.barcode.looking_up"]
args = { position = "Int", total = "Int" }
value = "Looking up barcode {position} of {total}"
"#);
        let resw = windows_resw(&c, "en", "en");
        assert!(
            resw.contains("name=\"core.identify.barcode.looking_up\""),
            "{resw}"
        );
        assert!(
            resw.contains("Looking up barcode {position} of {total}"),
            "{resw}"
        );
    }

    #[test]
    fn resx_names_source_flat_and_fans_out_per_culture() {
        let c = cat(r#"
[messages."core.error.not_found.release"]
value = "that release couldn't be found"
translations = { ar = "تعذر العثور على ذلك الإصدار", "zh-Hans" = "找不到该发行版" }
"#);
        let files = resx_all(&c, "en");
        // Core.resx (invariant source) + one Core.<culture>.resx per target locale.
        assert_eq!(files.len(), 3);
        let paths: Vec<String> = files
            .iter()
            .map(|(p, _)| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.contains(&"Core.resx".to_string()), "{paths:?}");
        assert!(paths.contains(&"Core.ar.resx".to_string()), "{paths:?}");
        assert!(
            paths.contains(&"Core.zh-Hans.resx".to_string()),
            "{paths:?}"
        );
        // The source file carries the English value; the culture file the
        // translation, both under the dotted id kept verbatim.
        let source = files
            .iter()
            .find(|(p, _)| p.to_string_lossy() == "Core.resx")
            .unwrap();
        assert!(
            source.1.contains("that release couldn't be found"),
            "{}",
            source.1
        );
        assert!(
            source.1.contains("name=\"core.error.not_found.release\""),
            "{}",
            source.1
        );
        let ar = files
            .iter()
            .find(|(p, _)| p.to_string_lossy() == "Core.ar.resx")
            .unwrap();
        assert!(ar.1.contains("تعذر العثور على ذلك الإصدار"), "{}", ar.1);
    }

    #[test]
    fn windows_fans_out_to_every_shipping_locale() {
        let c = cat(r#"
[messages."core.error.not_found.release"]
value = "that release couldn't be found"
translations = { ar = "تعذر العثور على ذلك الإصدار", "zh-Hans" = "找不到该发行版" }
"#);
        let files = windows_resw_all(&c, "en");
        // One Core.resw per generated locale: en-US source + catalog targets.
        assert_eq!(files.len(), 3);
        let paths: Vec<String> = files
            .iter()
            .map(|(p, _)| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.contains(&"en-US/Core.resw".to_string()), "{paths:?}");
        assert!(paths.contains(&"ar/Core.resw".to_string()), "{paths:?}");
        assert!(
            paths.contains(&"zh-Hans/Core.resw".to_string()),
            "{paths:?}"
        );
        let source = files
            .iter()
            .find(|(p, _)| p.to_string_lossy().replace('\\', "/") == "en-US/Core.resw")
            .unwrap();
        assert!(
            source.1.contains("that release couldn't be found"),
            "{}",
            source.1
        );
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
        let files = android_resource_files(&c, "en");
        // One source `values/` file plus one per catalog target locale.
        assert_eq!(files.len(), 3);
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"values/core_strings.xml"), "{paths:?}");
        assert!(
            paths.contains(&"values-b+zh+Hans/core_strings.xml"),
            "{paths:?}"
        );
        assert!(paths.contains(&"values-ar/core_strings.xml"), "{paths:?}");
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
        let json = apple_xcstrings(&c, "en").unwrap();
        assert!(json.contains("no se encontró ese lanzamiento"), "{json}");
        assert!(json.contains("%lld usunięcia oczekują"), "{json}");
        assert!(json.contains("%lld usunięć oczekuje"), "{json}");

        // Android: the Spanish file carries the translation verbatim; a target
        // locale untranslated for this message keeps the English source.
        let files = android_resource_files(&c, "en");
        let es = files
            .iter()
            .find(|(p, _)| p == "values-es/core_strings.xml")
            .unwrap();
        assert!(es.1.contains("no se encontró ese lanzamiento"), "{}", es.1);
        let pl = files
            .iter()
            .find(|(p, _)| p == "values-pl/core_strings.xml")
            .unwrap();
        assert!(pl.1.contains("that release couldn"), "{}", pl.1);

        // Windows: the Polish Core.resw carries the four-category MF1 verbatim.
        let resw = windows_resw_all(&c, "en");
        let pl = resw
            .iter()
            .find(|(p, _)| p.to_string_lossy().replace('\\', "/") == "pl/Core.resw")
            .unwrap();
        assert!(pl.1.contains("many {# usunięć oczekuje}"), "{}", pl.1);
    }
}
