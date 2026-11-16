//! The restore code a person's own device shows to add their new device.
//!
//! No restore code holds an S3 key (coven §13.1): the new device asks for it.

/// What this device shows for its person's restore code: the code to type or
/// copy, its groups as the masked code shows them, and the code for the QR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreCodeShare {
    pub code: String,
    pub masked_groups: Vec<String>,
    pub qr_text: String,
}

impl RestoreCodeShare {
    /// Show `code` to type, and `qr_text`, the code coven made for the QR.
    pub fn new(code: String, qr_text: String) -> Self {
        let code = grouped(&code);
        Self { masked_groups: masked_groups(&code), code, qr_text }
    }
}

/// How many characters of a code show before the next space.
const GROUP: usize = 4;

/// How many groups a masked code shows before its hidden ones: the prefix
/// and the first four characters.
const MASK_SHOWS_FIRST: usize = 2;

/// What a masked code shows in place of each hidden group.
const HIDDEN_GROUP: &str = "••••";

/// The groups of `grouped`, a code in groups of four, as the masked code
/// shows them: the first two and the last as they are, and each group between
/// as dots. A person can tell one code from another by them, and nobody can
/// read the code over their shoulder.
fn masked_groups(grouped: &str) -> Vec<String> {
    let groups: Vec<&str> = grouped.split(' ').collect();
    let last = groups.len() - 1;
    groups
        .iter()
        .enumerate()
        .map(
            |(index, group)| {
                if index < MASK_SHOWS_FIRST || index == last {
                    group
                } else {
                    HIDDEN_GROUP
                }
            },
        )
        .map(str::to_string)
        .collect()
}

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
