/// Parse an RFC 3339 timestamp into Unix epoch milliseconds — the one place that
/// converts coven's and bae's stored RFC 3339 text into the instant the UI wants.
/// The parse error is returned, not swallowed, so each caller decides what an
/// unparseable value means (log-and-drop, or a surfaced conversion error).
pub fn rfc3339_to_epoch_millis(s: &str) -> Result<i64, chrono::ParseError> {
    chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fractional-second timestamp keeps its sub-second precision, a
    /// whole-second one converts cleanly, and an unparseable string surfaces the
    /// parse error rather than a wrong instant.
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
