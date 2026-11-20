use super::super::*;

mirror_struct! {
    BridgeArtistCredit = bae_catalogs::credits::ArtistCredit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { name, sort_name, musicbrainz_artist_id, discogs_artist_id, credited_as },
}

mirror_enum! {
    BridgeArtistAssignment = bae_import::ArtistAssignment,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Picked { artist: (BridgeExistingArtist), credit_name: (opt BridgeCreditName) },
        Credit { credit: (BridgeArtistCredit) },
    },
}

mirror_struct! {
    BridgeCredited = bae_import::Credited,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { artist: (BridgeArtistAssignment), join },
}

mirror_enum! {
    BridgeArtistStanding = bae_import::ArtistStanding,
    from_core: fn,
    variants: {
        Library,
        New,
        Choose { choices: (each BridgeExistingArtist) },
    },
}

mirror_enum! {
    BridgeArtistsStanding = bae_import::ArtistsStanding,
    from_core: fn,
    variants: {
        Library,
        New,
        SomeNew { count },
        Choose { choices },
        SomeToChoose { count },
    },
}

mirror_struct! {
    BridgeArtistField = bae_import::ArtistField,
    from_core: pub(crate) fn,
    fields: { standing: (BridgeArtistsStanding), credits: (each BridgeStandingCredit) },
}

mirror_struct! {
    BridgeStandingCredit = bae_import::StandingCredit,
    from_core: fn,
    fields: { credited: (BridgeCredited), standing: (BridgeArtistStanding) },
}

mirror_enum! {
    BridgeTrackArtistAssignments = bae_import::TrackArtistAssignments,
    from_core: pub(super) fn,
    into_core: pub(crate) fn,
    variants: {
        AlbumArtists,
        Explicit(assignments: (each BridgeCredited)),
    },
}

mirror_struct! {
    BridgeTrackUserEdit = bae_import::TrackUserEdit,
    from_core: pub(crate) fn,
    into_core: fn,
    fields: {
        title,
        artist_assignments: (BridgeTrackArtistAssignments),
        file: (opt BridgeAudioFile),
    },
}

mirror_struct! {
    BridgeReleaseUserEdit = bae_import::ReleaseUserEdit,
    from_core: pub(crate) fn,
    fields: {
        album_title,
        album_artist_assignments: (each BridgeCredited),
        album_year,
        pressing: (BridgePressingEdit),
        tracks: (each BridgeTrackUserEdit),
    },
}

/// Not a `mirror_struct`: the editor draws the rows core lays out
/// ([`bae_import::RawPressingEdit::label_rows`]) and a form's rows come back
/// from them.
impl BridgeRawPressingEdit {
    pub(crate) fn from_core(pressing: bae_import::RawPressingEdit) -> Self {
        Self {
            labels: pressing.label_rows().into_iter().map(BridgeLabelRow::from_core).collect(),
            barcodes: pressing
                .barcode_rows()
                .into_iter()
                .map(BridgeBarcodeRow::from_core)
                .collect(),
            year: pressing.year,
            facts: BridgePressingFacts::from_core(pressing.facts),
        }
    }

    pub(crate) fn into_core(self) -> bae_import::RawPressingEdit {
        bae_import::RawPressingEdit {
            year: self.year,
            labels: self.labels.into_iter().map(BridgeLabelRow::into_core).collect(),
            barcodes: self.barcodes.into_iter().map(|row| row.barcode).collect(),
            facts: self.facts.into_core(),
        }
    }
}

impl BridgeListPlace {
    fn of_row<T>(row: &bae_import::ListRow<T>) -> Self {
        Self { labelled: row.labelled, adds_row: row.adds_row }
    }
}

impl BridgeLabelRow {
    fn from_core(row: bae_import::ListRow<bae_import::RawLabelEdit>) -> Self {
        let place = BridgeListPlace::of_row(&row);
        Self {
            label: BridgeRawLabelEdit {
                name: row.row.name,
                catalog_number: row.row.catalog_number,
            },
            place,
        }
    }

    fn into_core(self) -> bae_import::RawLabelEdit {
        self.label.into_core()
    }
}

impl BridgeBarcodeRow {
    fn from_core(row: bae_import::ListRow<String>) -> Self {
        let place = BridgeListPlace::of_row(&row);
        Self { barcode: row.row, place }
    }
}

/// The editor shows and sends a label row's name and number alone: the ids a
/// catalog gave the row stay in the core, which gives them back to a row that
/// keeps the name they came with.
impl BridgeRawLabelEdit {
    pub(crate) fn into_core(self) -> bae_import::RawLabelEdit {
        bae_import::RawLabelEdit {
            name: self.name,
            catalog_number: self.catalog_number,
            ..Default::default()
        }
    }
}

mirror_struct! {
    BridgeRawTrackEdit = bae_import::RawTrackEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        id,
        title,
        artist_assignments: (BridgeTrackArtistAssignments),
        file: (opt BridgeAudioFile),
    },
}

mirror_struct! {
    BridgePressingOffers = bae_import::PressingOffers,
    from_core: pub(crate) fn,
    fields: {
        media: (each BridgeMedium),
        discogs_details: (each BridgeDiscogsDetail),
    },
}

mirror_struct! {
    BridgeRawReleaseEdit = bae_import::RawReleaseEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        album_title,
        album_artist_assignments: (each BridgeCredited),
        album_year,
        pressing: (BridgeRawPressingEdit),
        tracks: (each BridgeRawTrackEdit),
    },
}

mirror_struct! {
    #[cfg(desktop)]
    BridgeReleaseEditDraft = bae_import::ReleaseEditDraft,
    from_core: pub(crate) fn,
    fields: {
        cover: (opt BridgeImageRef),
        source_audio: (opt BridgeSourceAudioSummary),
        sides: (opt each BridgeTrackTableSide),
        files: (opt each BridgeMappingFile),
        album: (BridgeDraftAlbum),
        records: (opt each BridgeReleaseRecord),
        pressing_offers: (BridgePressingOffers),
        album_artists: (opt BridgeArtistField),
        changed,
        files_to_read,
        invalid: (opt BridgeValidationReason),
    },
}
