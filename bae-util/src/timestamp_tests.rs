use super::*;

fn at(text: &str) -> UtcDateTime {
    parse_rfc3339(text).expect("the test date-time parses")
}

#[test]
fn rfc3339_text_writes_the_fraction_in_groups_of_three_digits() {
    assert_eq!(at("2024-01-01T12:34:56Z").to_rfc3339(), "2024-01-01T12:34:56+00:00");
    assert_eq!(at("2024-01-01T12:34:56.1Z").to_rfc3339(), "2024-01-01T12:34:56.100+00:00");
    assert_eq!(at("2024-01-01T12:34:56.1234Z").to_rfc3339(), "2024-01-01T12:34:56.123400+00:00");
    assert_eq!(
        at("2024-01-01T12:34:56.123456789Z").to_rfc3339(),
        "2024-01-01T12:34:56.123456789+00:00"
    );
}

#[test]
fn rfc3339_text_reads_back_as_the_same_date_time() {
    let original = at("2026-11-17T07:50:01.000002+02:00");
    assert_eq!(original.to_rfc3339(), "2026-11-17T05:50:01.000002+00:00");
    assert_eq!(parse_rfc3339(&original.to_rfc3339()).unwrap(), original);
}

#[test]
fn unix_milliseconds_round_down_and_read_back() {
    let before_epoch = at("1969-12-31T23:59:59.9995Z");
    assert_eq!(before_epoch.unix_timestamp_millis(), -1);
    let later = at("2023-11-14T22:13:20.123456Z");
    assert_eq!(later.unix_timestamp_millis(), 1_700_000_000_123);
    assert_eq!(
        from_unix_timestamp_millis(1_700_000_000_123).unwrap(),
        at("2023-11-14T22:13:20.123Z")
    );
}
