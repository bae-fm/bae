//! XML text escaping.

/// Escape the five XML special characters (`&`, `<`, `>`, `"`, `'`) so any text
/// is safe inside element content and attribute values. Everything else,
/// including non-ASCII text, passes through unchanged.
pub fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
#[path = "xml_tests.rs"]
mod tests;
