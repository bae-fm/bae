//! When two catalog numbers are one number, and where a line of text can
//! print one.

use super::squash;

/// A catalog number as two of them are compared: squashed, and `None` when
/// that leaves no number. Both catalogs write a release's lack of one as a
/// placeholder — `[none]` on MusicBrainz, `none` on Discogs, and in their
/// data also `None`, `(none)`, `- none` and `-none-` — which all squash to
/// `none`.
pub fn catalog_key(stated: &str) -> Option<String> {
    let key = squash(stated);
    (!key.is_empty() && key != "none").then_some(key)
}

/// The text cut where a catalog number printed in it can begin or end,
/// each piece squashed. A hyphen, dot or slash with a letter or digit on
/// both sides joins what it stands between into one number, so
/// `7559-61571-2` is one piece and neither `61571-2` nor `7559` is
/// printed there. Anything else that is not a letter or a digit — a
/// space, a bracket, a separator with a space beside it — cuts the text,
/// so a number printed with spaces, like `MCD 11600`, is a run of pieces
/// that together print it.
///
/// Run together the pieces are [`squash`] of the text, as
/// [`super::words`] are: each piece is one or more of its words.
#[cfg(desktop)]
pub fn catalog_words(text: &str) -> Vec<String> {
    printed_pieces(text).into_iter().map(|(_, piece)| super::squash(piece)).collect()
}

/// The catalog numbers `text` prints, as it writes them, each once in
/// first-seen order: what a folder's text offers to be picked. Read by
/// the cut [`catalog_words`] makes, so an offered number is always whole
/// numbers of its line, and the text prints it by
/// `CandidateText::prints_catalog`.
///
/// None of these is a number:
///
/// - A copyright line's mark and year, `P 2007`.
/// - An ISRC: two letters, three letters or digits, two digits, five digits.
/// - A hex digest of 8, 32, 40 or 64 digits that holds a letter and a
///   digit, as a ripper's report prints its checksums.
/// - A number a `%` follows, as `CDDA 100%`, or a version of one or two
///   digits, a dot and one or two digits, as `0.11`.
/// - A composer's work number, `BWV 810`, `BWV846`.
/// - A store's image size between underscores, `_SS500_`.
///
/// A number is a piece that begins with a capital letter, writes every
/// letter in capitals and holds three digits or more — `WPCR-80001`,
/// `CDP7.46437`, `BVCK-15024/5`. A piece of capitals and at most two
/// digits followed by one space and a piece that begins with a digit is
/// one number with it — `SD 19244-2`, `Z1 12345` — as long as the two
/// hold three digits or more. Separators are kept as written, since
/// MusicBrainz indexes `WPCR-80001` and `WPCR 80001` apart.
#[cfg(desktop)]
pub fn printed_catalog_numbers(text: &str) -> Vec<String> {
    let pieces = printed_pieces(text);
    let mut numbers: Vec<String> = Vec::new();
    let mut at = 0;
    while at < pieces.len() {
        let (start, piece) = pieces[at];
        let joined = pieces.get(at + 1).filter(|(next_start, next)| {
            is_prefix(piece)
                && text[start + piece.len()..*next_start] == *" "
                && next.starts_with(|c: char| c.is_ascii_digit())
                && digits(piece) + digits(next) >= 3
                && !is_copyright_year(piece, next)
                && !is_version(next)
        });
        let number = match joined {
            Some((next_start, next)) => {
                at += 2;
                Some((start, &text[start..next_start + next.len()]))
            }
            None => {
                at += 1;
                (is_capitals(piece) && digits(piece) >= 3).then_some((start, piece))
            }
        };
        let number = number.filter(|&(begin, number)| {
            let end = begin + number.len();
            !text[end..].starts_with('%')
                && !is_work_number(number)
                && !is_recording_code(number)
                && !is_digest(number)
                && !is_image_size(text, begin, number)
        });
        if let Some((_, number)) = number {
            if !numbers.iter().any(|seen| seen == number) {
                numbers.push(number.to_string());
            }
        }
    }
    numbers
}

/// Whether `piece` and `next` are a copyright line's mark and year: the
/// letter P (phonographic) or C, then four digits that are a year, as in
/// `P 2007`.
#[cfg(desktop)]
fn is_copyright_year(piece: &str, next: &str) -> bool {
    matches!(piece, "P" | "C")
        && next.len() == 4
        && next.chars().all(|c| c.is_ascii_digit())
        && (next.starts_with("19") || next.starts_with("20"))
}

/// Whether `piece` is a program's version: one or two digits, a dot, one or
/// two digits, as `0.11`.
#[cfg(desktop)]
fn is_version(piece: &str) -> bool {
    piece.split_once('.').is_some_and(|(major, minor)| {
        [major, minor]
            .iter()
            .all(|part| (1..=2).contains(&part.len()) && part.chars().all(|c| c.is_ascii_digit()))
    })
}

/// Whether `number` is an ISRC.
#[cfg(desktop)]
fn is_recording_code(number: &str) -> bool {
    let bytes = number.as_bytes();
    bytes.len() == 12
        && bytes[..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..5].iter().all(u8::is_ascii_alphanumeric)
        && bytes[5..].iter().all(u8::is_ascii_digit)
}

/// Whether `number` is a hex digest of 8, 32, 40 or 64 digits holding a
/// letter and a digit.
#[cfg(desktop)]
fn is_digest(number: &str) -> bool {
    matches!(number.len(), 8 | 32 | 40 | 64)
        && number.chars().all(|c| c.is_ascii_hexdigit())
        && number.chars().any(|c| c.is_ascii_alphabetic())
        && number.chars().any(|c| c.is_ascii_digit())
}

/// Whether `number` is a composer's work number: `BWV` and its digits.
#[cfg(desktop)]
fn is_work_number(number: &str) -> bool {
    number
        .strip_prefix("BWV")
        .is_some_and(|rest| rest.trim_start().starts_with(|c: char| c.is_ascii_digit()))
}

/// Whether `number`, which begins at `start` in `text`, is a store's image
/// size, `SS500`, `SX300`, `SY450` or `SL1500` between underscores.
#[cfg(desktop)]
fn is_image_size(text: &str, start: usize, number: &str) -> bool {
    number.len() >= 4
        && number.starts_with('S')
        && matches!(number.as_bytes()[1], b'S' | b'X' | b'Y' | b'L')
        && number[2..].chars().all(|c| c.is_ascii_digit())
        && text[..start].ends_with('_')
        && text[start + number.len()..].starts_with('_')
}

/// The text cut as [`catalog_words`] cuts it, each piece as written with
/// the byte it begins at.
#[cfg(desktop)]
fn printed_pieces(text: &str) -> Vec<(usize, &str)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let plain: Vec<char> = chars.iter().map(|(_, c)| *c).collect();
    let mut pieces = Vec::new();
    let mut start: Option<usize> = None;
    for (at, &(byte, c)) in chars.iter().enumerate() {
        if c.is_alphanumeric() {
            start.get_or_insert(byte);
        } else if !(joins(c) && joined(&plain, at)) {
            if let Some(begun) = start.take() {
                pieces.push((begun, &text[begun..byte]));
            }
        }
    }
    if let Some(begun) = start {
        pieces.push((begun, &text[begun..]));
    }
    pieces
}

/// Whether `piece` begins with a capital letter and writes no letter in
/// lower case.
#[cfg(desktop)]
fn is_capitals(piece: &str) -> bool {
    piece.starts_with(|c: char| c.is_ascii_uppercase()) && !piece.chars().any(char::is_lowercase)
}

/// Whether `piece` can begin a number printed with a space inside it:
/// capitals and at most two digits, joined by nothing.
#[cfg(desktop)]
fn is_prefix(piece: &str) -> bool {
    is_capitals(piece) && digits(piece) <= 2 && piece.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(desktop)]
fn digits(piece: &str) -> usize {
    piece.chars().filter(char::is_ascii_digit).count()
}

/// A mark that keeps a number whole when letters or digits stand on both
/// sides of it: `WPCR-80001`, `CDP7.46437`, `AB12/2`, and the Unicode
/// hyphens a sleeve's typesetting may print for `-`.
#[cfg(desktop)]
fn joins(c: char) -> bool {
    matches!(c, '-' | '.' | '/' | '\u{2010}' | '\u{2011}')
}

/// Whether the characters on both sides of `at` are letters or digits.
#[cfg(desktop)]
fn joined(chars: &[char], at: usize) -> bool {
    at.checked_sub(1).and_then(|before| chars.get(before)).is_some_and(|c| c.is_alphanumeric())
        && chars.get(at + 1).is_some_and(|c| c.is_alphanumeric())
}

#[cfg(test)]
#[path = "catalog_number_tests.rs"]
mod tests;
