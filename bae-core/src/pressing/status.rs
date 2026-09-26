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

desktop_only! {
    impl ReleaseStatus {
        /// The status a MusicBrainz release's `status` names. `None` for a name
        /// outside the list, which the caller logs: the list is what needs the
        /// new name.
        pub(crate) fn musicbrainz(name: &str) -> Option<Self> {
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
    const MUSICBRAINZ_RELEASE_STATUSES: &[(&str, ReleaseStatus)] = &[
        ("Official", ReleaseStatus::Official),
        ("Promotion", ReleaseStatus::Promotion),
        ("Bootleg", ReleaseStatus::Bootleg),
        ("Pseudo-Release", ReleaseStatus::PseudoRelease),
        ("Withdrawn", ReleaseStatus::Withdrawn),
        ("Cancelled", ReleaseStatus::Cancelled),
        ("Expunged", ReleaseStatus::Expunged),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_musicbrainzs_own_list() {
        assert_eq!(
            MUSICBRAINZ_RELEASE_STATUSES
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            super::super::recorded(include_str!(
                "../../test-fixtures/pressing-vocabulary/musicbrainz-release-statuses.txt"
            ))
        );
        let mut statuses: Vec<_> = MUSICBRAINZ_RELEASE_STATUSES
            .iter()
            .map(|(_, status)| *status)
            .collect();
        statuses.sort_by_key(|status| status.key());
        let mut all = ReleaseStatus::ALL.to_vec();
        all.sort_by_key(|status| status.key());
        assert_eq!(statuses, all, "every status is one MusicBrainz names");
    }

    #[test]
    fn a_name_is_read_whole_and_in_any_case() {
        assert_eq!(
            ReleaseStatus::musicbrainz("Pseudo-Release"),
            Some(ReleaseStatus::PseudoRelease)
        );
        assert_eq!(
            ReleaseStatus::musicbrainz("promotion"),
            Some(ReleaseStatus::Promotion)
        );
        assert_eq!(ReleaseStatus::musicbrainz("Promo"), None);
    }
}
