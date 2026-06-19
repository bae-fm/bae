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

/// The shipping locales beyond the English source. The generated catalog carries
/// a slot for each — English value, marked `new` (needs translation) — so the app
/// declares support and translators have a target; English shows at runtime until
/// a locale is actually translated. (Source: en. RTL: ar, he. CJK: ja, zh-Hans.
/// Slavic: uk/bg Cyrillic, pl/cs/hr Latin.)
const TARGET_LOCALES: &[&str] = &[
    "es", "fr", "de", "pt-BR", "ja", "zh-Hans", "ar", "he", "uk", "bg", "pl", "cs", "hr",
];

fn string_unit_state(value: &str, state: &str) -> serde_json::Value {
    serde_json::json!({ "stringUnit": { "state": state, "value": value } })
}

/// Build the per-locale `localizations` map: the English source at state
/// `translated`, and every `TARGET_LOCALES` entry at state `new`, each rendered
/// by `unit(state)`.
fn per_locale(src_lang: &str, unit: impl Fn(&str) -> serde_json::Value) -> serde_json::Value {
    let mut locs = serde_json::Map::new();
    locs.insert(src_lang.to_string(), unit("translated"));
    for loc in TARGET_LOCALES {
        locs.insert((*loc).to_string(), unit("new"));
    }
    serde_json::Value::Object(locs)
}

/// Build the `.xcstrings` `localizations` body for one message: the English
/// source (state `translated`) plus a slot for every `TARGET_LOCALES` entry
/// carrying the English value at state `new` — so the app declares the locale
/// and Xcode/translators see it as needing translation. English shows at runtime
/// until a slot is actually translated.
fn apple_localization(msg: &Message, src_lang: &str) -> Result<serde_json::Value, String> {
    let nodes = mf1::parse(&msg.value)?;
    let order = ordered_args(&nodes);

    // Whole-message plural -> variations.plural. Other plural shapes are errors.
    if let [Node::Plural { arg, cases }] = nodes.as_slice() {
        if msg.args.len() != 1 || !msg.args.contains_key(arg) {
            return Err(format!(
                "plural message `{}` must take exactly its count arg `{arg}`",
                msg.value
            ));
        }
        // Render each category once; the per-locale slots reuse the rendered
        // text (a translator-facing English starting point for that locale).
        let mut rendered_cases: Vec<(String, String)> = Vec::new();
        for case in cases {
            let cat = match &case.selector {
                PluralSelector::Category(c) => c.as_cldr(),
                PluralSelector::Exact(_) => {
                    return Err("`=N` exact plural cases are not supported on Apple yet".to_string())
                }
            };
            let rendered = apple_flat(&case.message, msg, &order, Some(arg))?;
            rendered_cases.push((cat.to_string(), rendered));
        }
        let plural_unit = |state: &str| -> serde_json::Value {
            let variations: serde_json::Map<String, serde_json::Value> = rendered_cases
                .iter()
                .map(|(cat, r)| (cat.clone(), string_unit_state(r, state)))
                .collect();
            serde_json::json!({ "variations": { "plural": variations } })
        };
        return Ok(per_locale(src_lang, plural_unit));
    }

    if nodes.iter().any(|n| matches!(n, Node::Plural { .. })) {
        return Err(format!(
            "message `{}` embeds a plural mid-text; Apple needs a substitution, not yet supported",
            msg.value
        ));
    }

    let rendered = apple_flat(&nodes, msg, &order, None)?;
    Ok(per_locale(src_lang, |state| {
        string_unit_state(&rendered, state)
    }))
}

/// Emit the source-language `Core.xcstrings` for the whole catalog.
pub fn apple_xcstrings(cat: &Catalog, src_lang: &str) -> Result<String, String> {
    let mut strings = serde_json::Map::new();
    for (id, msg) in &cat.messages {
        let mut entry = serde_json::Map::new();
        if let Some(comment) = &msg.comment {
            entry.insert("comment".to_string(), json_str(comment));
        }
        entry.insert(
            "localizations".to_string(),
            apple_localization(msg, src_lang)?,
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

/// Emit `core_strings.xml`. Every message is a plain `<string>`; the MF1 value
/// is stored verbatim (escaped) for `android.icu.text.MessageFormat`.
pub fn android_strings_xml(cat: &Catalog) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n");
    for (id, msg) in &cat.messages {
        out.push_str(&format!(
            "    <string name=\"{}\">{}</string>\n",
            sanitize_android_id(id),
            android_escape(&msg.value),
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

/// Emit the full Android resource set: the English source under `values/`, plus
/// one `values-<qualifier>/core_strings.xml` per `TARGET_LOCALES` entry carrying
/// the same English values. Android resolves resources per directory, so a
/// locale the app should "support" must have its own directory — without it the
/// locale falls back to `values/` and never registers as supported (so its
/// plural rules and right-to-left layout selection don't apply). The per-locale
/// files are English until a locale is actually translated, mirroring how the
/// Apple emitter writes an English-valued slot for every locale into the one
/// `Core.xcstrings`. Returns `(relative path, file contents)` pairs.
pub fn android_resource_files(cat: &Catalog) -> Vec<(String, String)> {
    let body = android_strings_xml(cat);
    let mut files = vec![("values/core_strings.xml".to_string(), body.clone())];
    for loc in TARGET_LOCALES {
        files.push((
            format!("{}/core_strings.xml", android_values_dir(loc)),
            body.clone(),
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

/// Emit `Core.resw`. The dotted id is the resource name; the MF1 value is stored
/// verbatim (XML-escaped) for the `MessageFormat` NuGet at runtime.
pub fn windows_resw(cat: &Catalog) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n  \
         <resheader name=\"resmimetype\"><value>text/microsoft-resx</value></resheader>\n  \
         <resheader name=\"version\"><value>2.0</value></resheader>\n",
    );
    for (id, msg) in &cat.messages {
        out.push_str(&format!(
            "  <data name=\"{}\" xml:space=\"preserve\"><value>{}</value></data>\n",
            xml_escape(id),
            xml_escape(&msg.value),
        ));
    }
    out.push_str("</root>\n");
    out
}

/// The shipping locales in build/BCP-47 form: the English source plus every
/// `TARGET_LOCALES` entry. Windows resources are per-language directories (unlike
/// Apple's single multi-locale `.xcstrings`), so the catalog fans out to one
/// `Core.resw` per locale.
fn windows_locales(src_lang: &str) -> Vec<String> {
    let mut locales = vec![format!("{src_lang}-US")];
    locales.extend(TARGET_LOCALES.iter().map(|l| (*l).to_string()));
    locales
}

/// Emit one `<locale>/Core.resw` per shipping locale: `(relative path, contents)`
/// pairs the caller writes under the project's `Strings` directory. Every locale
/// carries the **English** value — the source locale because it's the source, the
/// others as the untranslated starting point (English shows at runtime until a
/// locale's `Core.resw` is actually translated). This declares locale support
/// (the per-language directories are what make the app multilingual) without
/// inventing translations. Mirrors the Apple emitter, which carries the same
/// per-locale slots inside one file.
pub fn windows_resw_all(cat: &Catalog, src_lang: &str) -> Vec<(std::path::PathBuf, String)> {
    let contents = windows_resw(cat);
    windows_locales(src_lang)
        .into_iter()
        .map(|locale| {
            (
                std::path::PathBuf::from(locale).join("Core.resw"),
                contents.clone(),
            )
        })
        .collect()
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
        // Every shipping locale gets a slot; the non-source ones are `new`.
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
        let xml = android_strings_xml(&c);
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
        let resw = windows_resw(&c);
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
    fn windows_fans_out_to_every_shipping_locale() {
        let c = cat(r#"
[messages."core.error.not_found.release"]
value = "that release couldn't be found"
"#);
        let files = windows_resw_all(&c, "en");
        // One Core.resw per shipping locale: en-US source + every TARGET_LOCALES.
        assert_eq!(files.len(), TARGET_LOCALES.len() + 1);
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
        // Every locale carries the English source (others untranslated until a
        // translator fills them) — no invented translations.
        for (_, contents) in &files {
            assert!(
                contents.contains("that release couldn't be found"),
                "{contents}"
            );
        }
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
"#);
        let files = android_resource_files(&c);
        // One source `values/` file plus one per target locale.
        assert_eq!(files.len(), 1 + TARGET_LOCALES.len());
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"values/core_strings.xml"), "{paths:?}");
        assert!(
            paths.contains(&"values-b+zh+Hans/core_strings.xml"),
            "{paths:?}"
        );
        assert!(paths.contains(&"values-ar/core_strings.xml"), "{paths:?}");
        // Every file carries the same (English) body until a locale is translated.
        assert!(files.iter().all(|(_, body)| body.contains("mono")));
    }
}
