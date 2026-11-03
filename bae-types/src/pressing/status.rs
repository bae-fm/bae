//! How official a release is.
//!
//! MusicBrainz states it for every release from a closed list; Discogs states
//! it only where a format description is one of those statuses. The list is
//! MusicBrainz's, name for name, and the test that compares it against the
//! captured table keeps the code and the record together.

super::vocabulary! {
    /// Whether the artist and their label stand behind a release, and whether
    /// it reached the public as planned.
    pub enum ReleaseStatus {
        /// Sanctioned by the artist or their label.
        Official => "official",
        /// A give-away, or a copy made to promote a release: an advance for
        /// reviewers, a copy for radio.
        Promotion => "promotion",
        /// Not sanctioned by the artist or their label.
        Bootleg => "bootleg",
        /// A transliteration or translation of another release's titles,
        /// which is no object of its own.
        PseudoRelease => "pseudo_release",
        /// Withdrawn from circulation after it was released.
        Withdrawn => "withdrawn",
        /// Disowned by the artist or label after it was released.
        Expunged => "expunged",
        /// Announced and then never released.
        Cancelled => "cancelled",
    }
}

#[cfg(desktop)]
impl ReleaseStatus {
    /// The status a MusicBrainz release's `status` names. `None` for a name
    /// outside the list, which the caller logs: the list is what needs the
    /// new name.
    pub fn musicbrainz(name: &str) -> Option<Self> {
        MUSICBRAINZ_RELEASE_STATUSES
            .iter()
            .find(|(stated, _)| super::same_name(stated, name))
            .map(|(_, status)| *status)
    }
}

/// Every `release_status` row of MusicBrainz, in id order. Source: the
/// server's attribute translations (`po/attributes.pot`, generated from the
/// `release_status` table), captured 2026-09-25. The web service answers
/// with these names.
#[cfg(desktop)]
const MUSICBRAINZ_RELEASE_STATUSES: &[(&str, ReleaseStatus)] = &[
    ("Official", ReleaseStatus::Official),
    ("Promotion", ReleaseStatus::Promotion),
    ("Bootleg", ReleaseStatus::Bootleg),
    ("Pseudo-Release", ReleaseStatus::PseudoRelease),
    ("Withdrawn", ReleaseStatus::Withdrawn),
    ("Cancelled", ReleaseStatus::Cancelled),
    ("Expunged", ReleaseStatus::Expunged),
];

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
