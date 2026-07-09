/// Parse an RFC 3339 timestamp into Unix epoch milliseconds. coven and bae's
/// own queue both store sync/created times as RFC 3339 text, but the UI only
/// needs an instant, so this is the one place that maps the text to epoch
/// millis. The parse result is returned so each caller decides how to handle a
/// value that won't parse (log-and-drop, or surface as a conversion error).
pub fn rfc3339_to_epoch_millis(s: &str) -> Result<i64, chrono::ParseError> {
    chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rfc3339_to_epoch_millis` is the single conversion the sync-time and
    /// outbox-row paths share. A fractional-second timestamp keeps its
    /// sub-second precision, a plain whole-second one converts cleanly, and an
    /// unparseable string surfaces the parse error rather than a wrong instant.
    #[test]
    fn rfc3339_to_epoch_millis_handles_fractional_and_invalid() {
        assert_eq!(
            rfc3339_to_epoch_millis("2024-01-02T03:04:05Z").unwrap(),
            1_704_164_645_000,
        );
        assert_eq!(
            rfc3339_to_epoch_millis("2024-01-02T03:04:05.250Z").unwrap(),
            1_704_164_645_250,
        );
        assert!(rfc3339_to_epoch_millis("not a timestamp").is_err());
    }
}
