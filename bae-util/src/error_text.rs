//! How an error that holds a transport failure prints its cause. reqwest's
//! own text is often only "error sending request for url (…)": the DNS, TLS
//! or connection failure behind it is a cause further down, which its
//! `Display` leaves out. Every cause is printed, so the failure names what
//! actually went wrong, and each URL in it is cut to its scheme and host, so a
//! query string or a path never prints.
//!
//! bae prints every `reqwest::Error` through [`CauseChain`]; the typed lint
//! `clippy::reqwest_error_display` (tools/typed-lints) fails the hook's clippy
//! wherever one prints with `Display` or `Debug` any other way.

use std::error::Error;
use std::fmt;

/// `error` and every cause below it, joined by ": ", each with its URLs cut
/// to scheme, host and port.
pub struct CauseChain<'a>(pub &'a (dyn Error + 'static));

impl fmt::Display for CauseChain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cause = Some(self.0);
        let mut separator = "";
        while let Some(error) = cause {
            write!(f, "{separator}{}", redact_urls(&error.to_string()))?;
            separator = ": ";
            cause = error.source();
        }
        Ok(())
    }
}

/// `Debug` for an error type that holds a `reqwest::Error`: the type's name
/// and its `Display` text, which prints reqwest's error through
/// [`CauseChain`]. reqwest's own `Debug` can hold a full URL, so such a type
/// writes its `Debug` with this instead of deriving it.
pub fn debug_as_text(
    f: &mut fmt::Formatter<'_>,
    name: &str,
    error: &dyn fmt::Display,
) -> fmt::Result {
    f.debug_tuple(name).field(&format_args!("{error}")).finish()
}

/// `text` with each URL cut to its scheme, host and port.
fn redact_urls(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) =
        ["http://", "https://"].iter().filter_map(|scheme| rest.find(scheme)).min()
    {
        redacted.push_str(&rest[..start]);
        let end = rest[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '"' | '\'' | '>'))
            .map_or(rest.len(), |length| start + length);
        match reqwest::Url::parse(&rest[start..end]) {
            Ok(url) => {
                redacted.push_str(url.scheme());
                redacted.push_str("://");
                if let Some(host) = url.host_str() {
                    redacted.push_str(host);
                }
                if let Some(port) = url.port() {
                    redacted.push_str(&format!(":{port}"));
                }
                redacted.push_str("/…");
            }
            Err(_) => redacted.push_str("<url>"),
        }
        rest = &rest[end..];
    }
    redacted.push_str(rest);
    redacted
}

#[cfg(test)]
#[path = "error_text_tests.rs"]
mod tests;
