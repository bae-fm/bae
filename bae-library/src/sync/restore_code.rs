//! The restore code a person's own device shows to add their new device.
//!
//! No restore code holds an S3 key (coven §13.1): the new device asks for it.

/// What this device shows for its person's restore code: the code to type or
/// copy, and the code for the QR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreCodeShare {
    pub code: String,
    pub qr_text: String,
}

impl RestoreCodeShare {
    /// Show `code` to type, and `qr_text`, the code coven made for the QR.
    pub fn new(code: String, qr_text: String) -> Self {
        Self { code: grouped(&code), qr_text }
    }
}

/// How many characters of a code show before the next space.
const GROUP: usize = 4;

/// A code in groups of four, its prefix (`CVR1`) the first: how the code to
/// type or copy shows and prints. [`entered`] reads it back.
pub fn grouped(code: &str) -> String {
    let compact: Vec<char> = code.chars().filter(|c| *c != '-' && !c.is_whitespace()).collect();
    compact
        .chunks(GROUP)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A code as a person entered it, typed or pasted, ready to decode: with the
/// spaces and line breaks of its groups dropped, and the dash after its
/// prefix, which the groups don't show, back. A code with no more than a
/// prefix is left as it is.
pub fn entered(text: &str) -> String {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    match compact.char_indices().nth(GROUP) {
        Some((at, next)) if next != '-' => format!("{}-{}", &compact[..at], &compact[at..]),
        _ => compact,
    }
}

#[cfg(test)]
#[path = "restore_code_tests.rs"]
mod tests;
