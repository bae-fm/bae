//! The forms a UTC date-time takes in bae's tables, its synced rows and the
//! text it sends: RFC 3339 text and whole milliseconds since the Unix epoch.
//!
//! bae writes the text form's offset as `+00:00`, and the fraction of a
//! second in groups of three digits: no fraction when the time falls on a
//! whole second, and otherwise 3, 6 or 9 digits, the fewest that hold the
//! time exactly. The rows that other devices synced, and the rows bae wrote
//! before, hold this form, and SQLite sorts a column of it as text in time
//! order, so every bae write keeps the form.

use std::fmt;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcDateTime};

/// The stored forms of a UTC date-time.
pub trait StoredTimestamp {
    /// The RFC 3339 text bae stores, with the offset written `+00:00`.
    fn to_rfc3339(&self) -> String;

    /// Whole milliseconds since the Unix epoch, rounded down.
    fn unix_timestamp_millis(&self) -> i64;
}

impl StoredTimestamp for UtcDateTime {
    fn to_rfc3339(&self) -> String {
        let mut text = String::with_capacity(32);
        write_rfc3339(&mut text, *self, "+00:00").expect("writing to a String never fails");
        text
    }

    fn unix_timestamp_millis(&self) -> i64 {
        let millis = self.unix_timestamp_nanos().div_euclid(1_000_000);
        i64::try_from(millis).expect("a UtcDateTime's milliseconds fit an i64")
    }
}

/// The date-time `millis` whole milliseconds after the Unix epoch.
pub fn from_unix_timestamp_millis(millis: i64) -> Result<UtcDateTime, time::error::ComponentRange> {
    UtcDateTime::from_unix_timestamp_nanos(i128::from(millis) * 1_000_000)
}

/// The UTC date-time that RFC 3339 `text` names, in any offset.
pub fn parse_rfc3339(text: &str) -> Result<UtcDateTime, time::error::Parse> {
    OffsetDateTime::parse(text, &Rfc3339).map(OffsetDateTime::to_utc)
}

/// Writes `at` as RFC 3339 text with `utc_offset` as its offset.
fn write_rfc3339(out: &mut impl fmt::Write, at: UtcDateTime, utc_offset: &str) -> fmt::Result {
    let year = at.year();
    if (0..=9999).contains(&year) {
        write!(out, "{year:04}")?;
    } else {
        // ISO 8601 requires a sign on a year outside 0000 to 9999.
        write!(out, "{year:+05}")?;
    }
    write!(
        out,
        "-{:02}-{:02}T{:02}:{:02}:{:02}",
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )?;
    let nanos = at.nanosecond();
    if nanos == 0 {
    } else if nanos.is_multiple_of(1_000_000) {
        write!(out, ".{:03}", nanos / 1_000_000)?;
    } else if nanos.is_multiple_of(1_000) {
        write!(out, ".{:06}", nanos / 1_000)?;
    } else {
        write!(out, ".{nanos:09}")?;
    }
    out.write_str(utc_offset)
}

/// Serde functions that write a UTC date-time field as RFC 3339 text with the
/// offset `Z`, and read the text back. A field names them with
/// `#[serde(with = "bae_util::timestamp::rfc3339_z")]`.
pub mod rfc3339_z {
    use super::{parse_rfc3339, write_rfc3339};
    use std::fmt;
    use time::UtcDateTime;

    pub fn serialize<S: serde::Serializer>(
        at: &UtcDateTime,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        struct Text(UtcDateTime);
        impl fmt::Display for Text {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_rfc3339(f, self.0, "Z")
            }
        }
        serializer.collect_str(&Text(*at))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<UtcDateTime, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        parse_rfc3339(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "timestamp_tests.rs"]
mod tests;
