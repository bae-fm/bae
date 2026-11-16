//! The notice bae shows after a copy behind another installation of its
//! device resets at once.

use bae_types::count_label::CountLabel;

/// The notice bae shows after a library finds its device id in use and
/// resets right away: its first sentence, then one sentence for each kind of
/// unsent work the reset discarded, in that order. The app looks up the words
/// of each sentence by its `Core` table key and joins them with a space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyResetNotice {
    pub intro_key: String,
    /// The discarded edits, then the discarded files; a kind with a count
    /// of zero has no sentence.
    pub counts: Vec<CountLabel>,
}

impl CopyResetNotice {
    /// The notice for a reset that discarded `unsent`, or `None` when the
    /// copy had no unsent work: bae then shows no notice.
    pub fn of(unsent: coven::UnsentWork) -> Option<Self> {
        let counts: Vec<CountLabel> = [
            CountLabel::nonzero("core.sync.copy_reset.edits", unsent.writes),
            CountLabel::nonzero("core.sync.copy_reset.files", unsent.files),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!counts.is_empty())
            .then(|| Self { intro_key: "core.sync.copy_reset.notice".to_string(), counts })
    }
}

#[cfg(test)]
#[path = "copy_reset_notice_tests.rs"]
mod tests;
