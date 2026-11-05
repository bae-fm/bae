//! The markup Discogs writes into a release's notes, read as the text it
//! shows.

/// `notes` as a reader of Discogs sees it. `[a=Name]` and `[l=Name]` show
/// `Name`; `[url=…]` and `[/url]` show nothing around the text between them;
/// `[b]`, `[i]` and `[u]` and their closing tags show nothing. A reference
/// that names only an id (`[a123]`, `[l123]`, `[r123]`, `[m123]`, `[r=123]`,
/// `[m=123]`) names nothing the note can show, and removing it would change
/// the sentence, so it stays as written, like any other bracketed text.
pub(crate) fn shown_text(notes: &str) -> String {
    let mut shown = String::with_capacity(notes.len());
    let mut rest = notes;
    while let Some(open) = rest.find('[') {
        shown.push_str(&rest[..open]);
        let from_open = &rest[open..];
        let Some(close) = from_open.find(']') else {
            rest = from_open;
            break;
        };
        // A `[` before the `]` starts the tag; the first one is text.
        let inner = &from_open[1..close];
        if inner.contains('[') {
            shown.push('[');
            rest = &from_open[1..];
            continue;
        }
        match tag_text(inner) {
            Some(text) => shown.push_str(text),
            None => shown.push_str(&from_open[..=close]),
        }
        rest = &from_open[close + 1..];
    }
    shown.push_str(rest);
    shown
}

/// What the tag `[inner]` shows, or `None` when it is not markup this reads.
fn tag_text(inner: &str) -> Option<&str> {
    match inner {
        "b" | "/b" | "i" | "/i" | "u" | "/u" | "url" | "/url" => return Some(""),
        _ => {}
    }
    if inner.starts_with("url=") {
        return Some("");
    }
    inner.strip_prefix("a=").or_else(|| inner.strip_prefix("l="))
}

#[cfg(test)]
#[path = "markup_tests.rs"]
mod tests;
