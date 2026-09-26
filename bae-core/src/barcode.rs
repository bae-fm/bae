//! UPC and EAN codes — the barcode printed on the back of a physical release —
//! and every way bae reads one.
//!
//! A code reaches bae three ways, and each is read here and nowhere else:
//!
//! * A machine states it — the artwork detector's payload, a CUE `CATALOG`
//!   field. [`Barcode::stated`] admits it when its check digit holds.
//! * It is printed as text — the digits under the bars, which the text
//!   recognizer reads as a line. [`Barcode::printed`] admits the line only when
//!   its digits are grouped the way the symbology prints them and the check
//!   digit holds.
//! * A catalog or a person writes it down — a provider record's barcode field,
//!   a typed search. [`written_digits`] reads the digits out of the spacing,
//!   and [`comparison_key`] is what two provider records are compared by.
//!
//! All three agree on one spelling: digits only, with a twelve-digit UPC-A
//! written as the thirteen-digit EAN-13 that prefixes a zero — the one
//! equivalence the encodings define.

/// A UPC or EAN code whose check digit holds, in its one spelling: thirteen
/// digits for an EAN-13 or UPC-A, eight for an EAN-8 or UPC-E.
///
/// The eight-digit forms are kept as printed. EAN-8 and UPC-E share the
/// length but not the numbering, and the providers index each as printed, so
/// neither is rewritten into the other or into its thirteen-digit expansion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Barcode(String);

impl Barcode {
    /// A code as a machine states it: a detector's payload, a CUE `CATALOG`
    /// field. Digits, optionally with spaces or hyphens between them, of a
    /// length some symbology has, and a check digit that holds for it.
    pub fn stated(value: &str) -> Option<Self> {
        let digits = written_digits(value)?;
        Symbology::ALL
            .into_iter()
            .any(|symbology| symbology.admits(&digits))
            .then(|| Self::spelled(digits))
            .filter(|code| !is_placeholder(&code.0))
    }

    /// A code read off a printed line: the human-readable digits under the
    /// bars, which a text recognizer returns as one line. The line must be
    /// the code and nothing else, split only where the symbology prints a
    /// gap — `8 012345 678900` for an EAN-13, `0 12345 67890 5` for a UPC-A —
    /// or at fewer of those places, since a recognizer may close a gap.
    ///
    /// The layout is what tells a code from a number that merely has a code's
    /// digits. A catalog number made from the barcode body, like
    /// `0946 3 12345 2 3`, is split where no barcode prints a gap: it is the
    /// catalog number, and the catalog-number reader is what reads it.
    pub fn printed(line: &str) -> Option<Self> {
        let groups: Vec<&str> = line
            .trim()
            .split([' ', '-'])
            .filter(|group| !group.is_empty())
            .collect();
        if groups.is_empty() || !groups.iter().all(|group| is_digits(group)) {
            return None;
        }
        let breaks: Vec<usize> = groups
            .iter()
            .scan(0, |position, group| {
                *position += group.len();
                Some(*position)
            })
            .take(groups.len() - 1)
            .collect();
        let digits = groups.concat();
        Symbology::ALL
            .into_iter()
            .any(|symbology| {
                breaks
                    .iter()
                    .all(|at| symbology.printed_breaks().contains(at))
                    && symbology.admits(&digits)
            })
            .then(|| Self::spelled(digits))
            .filter(|code| !is_placeholder(&code.0))
    }

    /// The code's digits in its one spelling — what a sighting records and a
    /// lookup asks for.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    fn spelled(digits: String) -> Self {
        Self(widened(digits))
    }
}

/// The digits of a value written as a code: digits, with spaces or hyphens
/// between them and nothing else. `None` for anything with other characters
/// in it — a word, a note beside the number — or with no digits at all.
///
/// No length or check digit is asked for: a provider record or a person may
/// write down a code that fails its check, and what they wrote is what is
/// compared or searched for.
pub fn written_digits(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_digit() || c == ' ' || c == '-')
    {
        return None;
    }
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

/// Why a provider's stated barcode is no key to compare by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unusable {
    /// Something other than a code was written in the barcode field.
    NotACode,
    /// Fewer digits than any UPC or EAN has.
    TooShort,
    /// A run of one digit — an unfilled field.
    Placeholder,
}

/// The key a provider record's stated barcode is compared with another
/// record's by: its written digits in the one spelling [`Barcode`] uses.
///
/// The check digit is not asked for. Two records stating the same mistyped
/// code state the same thing, and a record whose code fails its check still
/// differs from one stating another code.
pub fn comparison_key(stated: &str) -> Result<String, Unusable> {
    let digits = written_digits(stated).ok_or(Unusable::NotACode)?;
    if digits.len() < SHORTEST_CODE {
        return Err(Unusable::TooShort);
    }
    if is_placeholder(&digits) {
        return Err(Unusable::Placeholder);
    }
    Ok(widened(digits))
}

/// The fewest digits any UPC or EAN has — EAN-8 and UPC-E.
const SHORTEST_CODE: usize = 8;

/// A twelve-digit UPC-A written as the EAN-13 it is: the same number with a
/// leading zero. Every other length is kept as it is.
fn widened(digits: String) -> String {
    if digits.len() == 12 {
        format!("0{digits}")
    } else {
        digits
    }
}

/// A run of one digit, which is what an unfilled tag or CUE `CATALOG` field
/// holds (`0000000000000`). Some of these pass their check digit — all zeros
/// does — but none is printed on a product, and a lookup for one can only
/// miss.
fn is_placeholder(digits: &str) -> bool {
    let mut chars = digits.chars();
    chars.next().is_some_and(|first| chars.all(|c| c == first))
}

fn is_digits(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_digit())
}

/// The four symbologies music packaging carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Symbology {
    Ean13,
    UpcA,
    Ean8,
    UpcE,
}

impl Symbology {
    const ALL: [Symbology; 4] = [Self::Ean13, Self::UpcA, Self::Ean8, Self::UpcE];

    fn len(self) -> usize {
        match self {
            Self::Ean13 => 13,
            Self::UpcA => 12,
            Self::Ean8 | Self::UpcE => 8,
        }
    }

    /// Where the human-readable digits under the bars break into groups, as
    /// the number of digits before each break: an EAN-13's first digit, then
    /// two groups of six; a UPC-A's number-system digit, two groups of five,
    /// and its check digit; an EAN-8's two groups of four; a UPC-E's
    /// number-system digit, six digits, and its check digit.
    fn printed_breaks(self) -> &'static [usize] {
        match self {
            Self::Ean13 => &[1, 7],
            Self::UpcA => &[1, 6, 11],
            Self::Ean8 => &[4],
            Self::UpcE => &[1, 7],
        }
    }

    /// Whether `digits` is a code of this symbology: its length, and a check
    /// digit that holds.
    fn admits(self, digits: &str) -> bool {
        if digits.len() != self.len() || !is_digits(digits) {
            return false;
        }
        let (body, check) = digits.split_at(digits.len() - 1);
        let body: Vec<u8> = body.bytes().map(|byte| byte - b'0').collect();
        let check = check.as_bytes()[0] - b'0';
        match self {
            Self::Ean13 | Self::UpcA | Self::Ean8 => check_digit(&body) == check,
            Self::UpcE => upc_e_expansion(&body).is_some_and(|body| check_digit(&body) == check),
        }
    }
}

/// The GS1 check digit of a code's body: its digits weighted three and one
/// alternately from the rightmost, which weighs three.
fn check_digit(body: &[u8]) -> u8 {
    let sum: u32 = body
        .iter()
        .rev()
        .enumerate()
        .map(|(index, &digit)| u32::from(digit) * if index % 2 == 0 { 3 } else { 1 })
        .sum();
    ((10 - sum % 10) % 10) as u8
}

/// The UPC-A body (eleven digits) a UPC-E body (its number-system digit and
/// six more) stands for, which is what a UPC-E's check digit is computed
/// over. `None` for a number system other than 0 or 1, which UPC-E does not
/// encode.
fn upc_e_expansion(body: &[u8]) -> Option<Vec<u8>> {
    let [system, x1, x2, x3, x4, x5, x6] = *body else {
        return None;
    };
    if system > 1 {
        return None;
    }
    Some(match x6 {
        0..=2 => vec![system, x1, x2, x6, 0, 0, 0, 0, x3, x4, x5],
        3 => vec![system, x1, x2, x3, 0, 0, 0, 0, 0, x4, x5],
        4 => vec![system, x1, x2, x3, x4, 0, 0, 0, 0, 0, x5],
        _ => vec![system, x1, x2, x3, x4, x5, 0, 0, 0, 0, x6],
    })
}

/// `body` with its check digit appended — for a test that needs many
/// distinct codes rather than one written out.
#[cfg(test)]
pub(crate) fn with_check_digit(body: &str) -> String {
    let digits: Vec<u8> = body.bytes().map(|byte| byte - b'0').collect();
    format!("{body}{}", check_digit(&digits))
}

#[cfg(test)]
#[path = "barcode_tests.rs"]
mod tests;
