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

fn string_unit(value: &str) -> serde_json::Value {
    serde_json::json!({ "stringUnit": { "state": "translated", "value": value } })
}

/// Build the `.xcstrings` `localizations.<lang>` body for one message.
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
        let mut variations = serde_json::Map::new();
        for case in cases {
            let cat = match &case.selector {
                PluralSelector::Category(c) => c.as_cldr(),
                PluralSelector::Exact(_) => {
                    return Err("`=N` exact plural cases are not supported on Apple yet".to_string())
                }
            };
            let rendered = apple_flat(&case.message, msg, &order, Some(arg))?;
            variations.insert(cat.to_string(), string_unit(&rendered));
        }
        return Ok(serde_json::json!({
            src_lang: { "variations": { "plural": variations } }
        }));
    }

    if nodes.iter().any(|n| matches!(n, Node::Plural { .. })) {
        return Err(format!(
            "message `{}` embeds a plural mid-text; Apple needs a substitution, not yet supported",
            msg.value
        ));
    }

    let rendered = apple_flat(&nodes, msg, &order, None)?;
    Ok(serde_json::json!({ src_lang: string_unit(&rendered) }))
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
    fn android_id_sanitization() {
        assert_eq!(sanitize_android_id("core.a.b-c"), "core_a_b_c");
    }
}
