//! Convert a parsed catalog into each platform's native resource file.
//!
//! Apple is the only target that needs structural conversion: ICU MessageFormat
//! named args become positional `%lld`/`%@`, and a whole-message `plural`
//! becomes a String Catalog `variations.plural`. Android stores the MF1 string
//! **verbatim** (its runtime parses it) — the only work there is resource-file
//! escaping and sanitizing the dotted id into a legal resource name.
//!
//! Every target emits `TARGET_LOCALES` — the declared shipping set, not
//! whatever the catalog happens to carry — so a locale the app claims to support
//! always has a resource set. A message with no translation for one of them
//! emits the English source at state `new`; `check::locale_coverage` is what
//! keeps that from reaching a build.
//!
//! Only the message shapes bae actually authors are handled; anything else
//! (plural combined with extra args, a plural embedded mid-sentence) returns an
//! error so the build fails loudly instead of emitting wrong output.

use crate::mf1::{self, Node, PluralSelector};
use crate::{ArgType, Catalog, Message, SOURCE_LOCALE, TARGET_LOCALES};

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
fn localized<'a>(msg: &'a Message, locale: &str) -> (&'a str, &'static str) {
    if locale == SOURCE_LOCALE {
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
///
/// The positions the arguments take are the source message's, whatever order
/// a translation puts them in: a caller passes them in the order the English
/// states them, so a translation that says the second one first still names
/// it `%2$`.
fn apple_unit(msg: &Message, value: &str, state: &str) -> Result<serde_json::Value, String> {
    let nodes = mf1::parse(value)?;
    let order = ordered_args(&mf1::parse(&msg.value)?);

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
/// locale (state `translated`) plus every shipping target locale — its
/// translation at state `translated`, or the source value at `new` where that
/// locale isn't translated yet.
fn apple_localization(msg: &Message) -> Result<serde_json::Value, String> {
    let mut locs = serde_json::Map::new();
    locs.insert(SOURCE_LOCALE.to_string(), apple_unit(msg, &msg.value, "translated")?);
    for loc in TARGET_LOCALES {
        let (value, state) = localized(msg, loc);
        locs.insert((*loc).to_string(), apple_unit(msg, value, state)?);
    }
    Ok(serde_json::Value::Object(locs))
}

/// Emit the source-language `Core.xcstrings` for the whole catalog.
pub fn apple_xcstrings(cat: &Catalog) -> Result<String, String> {
    let mut strings = serde_json::Map::new();
    for (id, msg) in &cat.messages {
        let mut entry = serde_json::Map::new();
        if let Some(comment) = &msg.comment {
            entry.insert("comment".to_string(), json_str(comment));
        }
        entry.insert("localizations".to_string(), apple_localization(msg)?);
        strings.insert(id.clone(), serde_json::Value::Object(entry));
    }
    let doc = serde_json::json!({
        "sourceLanguage": SOURCE_LOCALE,
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
    let has_boundary_whitespace = s.chars().next().is_some_and(char::is_whitespace)
        || s.chars().next_back().is_some_and(char::is_whitespace);
    if has_boundary_whitespace {
        format!("\"{out}\"")
    } else {
        out
    }
}

/// Emit `core_strings.xml` for one `locale`. Every message is a plain
/// `<string>`; the MF1 value for that locale — its translation, or the English
/// source where untranslated — is stored verbatim (escaped) for
/// `android.icu.text.MessageFormat`.
pub fn android_strings_xml(cat: &Catalog, locale: &str) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n");
    for (id, msg) in &cat.messages {
        let (value, _state) = localized(msg, locale);
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
/// one `values-<qualifier>/core_strings.xml` per shipping target locale carrying
/// that locale's translations (source-language values where a message isn't
/// translated yet).
/// Android resolves resources per directory, so a locale the app should
/// "support" must have its own directory — without it the locale falls back to
/// `values/` and never registers as supported (so its plural rules and
/// right-to-left layout selection don't apply). Returns `(relative path, file
/// contents)` pairs.
pub fn android_resource_files(cat: &Catalog) -> Vec<(String, String)> {
    let mut files =
        vec![("values/core_strings.xml".to_string(), android_strings_xml(cat, SOURCE_LOCALE))];
    for loc in TARGET_LOCALES {
        files.push((
            format!("{}/core_strings.xml", android_values_dir(loc)),
            android_strings_xml(cat, loc),
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

#[cfg(test)]
#[path = "emit_tests.rs"]
mod tests;
