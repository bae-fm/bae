//! Which label a label's name names, without the trade word it trails.
//!
//! A folder writes "Label Name"; the sources write "Label Name Records".
//! That is one label. The word a company appends to its name — Records,
//! Recordings, Music, Ltd — says what kind of business it is and nothing about
//! which one it is, so ranking drops it before asking whether the candidate's
//! text states the label.
//!
//! A name that is nothing but trade words — "Records" — names no label at all,
//! and so states nothing about a result.
//!
//! A label may also be written as its initials — "DFC" for "Dance Floor
//! Corporation" — and two names agree when one is the other's initials.
//! [`LabelName::same_label`] is the one place two label names are compared:
//! pairing two records as one pressing, joining two catalogs' albums, and
//! listing a row's or a release's labels each once all ask it, directly or
//! through [`same_label_name`].

use std::sync::OnceLock;

/// One label's name, read into what the comparisons ask of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelName {
    /// Its words, with the trade words it ends on dropped, run together the
    /// way the candidate's text is read.
    stated: String,
    /// The first letters of those words, stop words left out, when they are
    /// two to four — "dfc" for "The Dance Floor Corporation Records".
    initials: Option<String>,
    /// What it is when it is written as initials: once the trade words it
    /// ends on are dropped, one word of two to four capitals — "DFC",
    /// "D.F.C." — lowercased. A word in lowercase, or longer, is a name.
    written_initials: Option<String>,
}

impl LabelName {
    /// `None` when nothing of the name is left once its trade words are
    /// dropped.
    pub fn of(name: &str) -> Option<Self> {
        let words = without_trade_words(super::words(name));
        if words.is_empty() {
            return None;
        }
        let initials: String = words
            .iter()
            .filter(|word| !super::is_stop_word(word))
            .filter_map(|word| word.chars().next())
            .collect();
        Some(Self {
            stated: words.concat(),
            initials: (2..=4).contains(&initials.chars().count()).then_some(initials),
            written_initials: written_initials(name),
        })
    }

    /// Whether the two name one label: they read alike, or one is written as
    /// the other's initials.
    pub fn same_label(&self, other: &Self) -> bool {
        self.stated == other.stated
            || self.written_as_initials_of(other)
            || other.written_as_initials_of(self)
    }

    fn written_as_initials_of(&self, other: &Self) -> bool {
        self.written_initials.is_some() && self.written_initials == other.initials
    }

    /// Its words run together, as a line of text is searched for it.
    #[cfg(desktop)]
    pub fn stated(&self) -> &str {
        &self.stated
    }

    /// Its initials, when its words make some.
    #[cfg(desktop)]
    pub fn initials(&self) -> Option<&str> {
        self.initials.as_deref()
    }

    /// The initials it is written as, when it is.
    #[cfg(desktop)]
    pub fn written_initials(&self) -> Option<&str> {
        self.written_initials.as_deref()
    }
}

/// Whether two written label names name one label, as
/// [`LabelName::same_label`] says. Two names that name no label — nothing but
/// trade words — are one only when they are written alike, as
/// [`super::normalize`] compares them.
pub fn same_label_name(a: &str, b: &str) -> bool {
    match (LabelName::of(a), LabelName::of(b)) {
        (Some(a), Some(b)) => a.same_label(&b),
        (None, None) => super::normalize(a) == super::normalize(b),
        _ => false,
    }
}

/// A label's name as a library label is found by it: its words with the
/// trade words it ends on dropped, run together ("Label Name Records" and "Label
/// Name" are one key). Empty for a name that is nothing but trade words, which
/// finds no label. Two initials and the name they abbreviate are two keys:
/// a key is one name's own, and two labels with one key are the same label
/// by name.
pub fn label_name_key(name: &str) -> String {
    LabelName::of(name).map(|name| name.stated).unwrap_or_default()
}

/// `words` without the trade words they end on.
fn without_trade_words(mut words: Vec<String>) -> Vec<String> {
    while let Some(tail) = tails().iter().find(|tail| words.ends_with(tail)) {
        words.truncate(words.len() - tail.len());
    }
    words
}

/// The initials `name` is written as, lowercased, when it is written as
/// initials — see [`LabelName::written_initials`].
fn written_initials(name: &str) -> Option<String> {
    let mut written = super::written_words(name);
    loop {
        let lowered: Vec<String> = written.iter().map(|word| word.to_lowercase()).collect();
        let Some(tail) = tails().iter().find(|tail| lowered.ends_with(tail)) else {
            break;
        };
        written.truncate(written.len() - tail.len());
    }
    let [word] = written.as_slice() else {
        return None;
    };
    let initials = (2..=4).contains(&word.chars().count()) && word.chars().all(char::is_uppercase);
    initials.then(|| word.to_lowercase())
}

/// Each trade word as the words it is written with, compared the way the
/// candidate's text is read — case, spacing and punctuation dropped, so
/// "Record Co." and "record co" are one entry. An entry that is written with
/// no words at all would end every name, and is left out. Built once.
fn tails() -> &'static [Vec<String>] {
    static TAILS: OnceLock<Vec<Vec<String>>> = OnceLock::new();
    TAILS.get_or_init(|| {
        TRADE_WORDS
            .iter()
            .map(|entry| super::words(entry))
            .filter(|tail| !tail.is_empty())
            .collect()
    })
}

/// The words a label's name trails that name no label of their own.
static TRADE_WORDS: &[&str] = &[
    "Records",
    "Recordings",
    "Record Co.",
    "Music",
    "Entertainment",
    "Productions",
    "Label",
    "Ltd",
    "Inc",
    "LLC",
    "GmbH",
];

#[cfg(test)]
#[path = "label_tests.rs"]
mod tests;
