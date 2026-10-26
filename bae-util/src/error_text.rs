//! How an error that holds a transport failure prints its cause. reqwest's
//! own text is often only "error sending request for url (…)": the DNS, TLS
//! or connection failure behind it is a cause further down, which its
//! `Display` leaves out. Every cause is printed, so the failure names what
//! actually went wrong, and each URL in it is cut to its scheme and host, so a
//! query string or a path never prints.
//!
//! Every bae error variant that holds a `reqwest::Error` prints it through
//! [`CauseChain`]; `scripts/check-reqwest-error-display.py` fails the build
//! when one prints it any other way.

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

/// `text` with each URL cut to its scheme, host and port.
fn redact_urls(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = ["http://", "https://"]
        .iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
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
