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
        Self { labelled: row.labelled, removable: row.removable, adds_row: row.adds_row }
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
    BridgeReleaseEditTrackSource = bae_import::ReleaseEditTrackSource,
    from_core: fn,
    fields: { file_id, name, layout: (BridgeSourceAudioLayout) },
}

mirror_struct! {
    BridgeReleaseEditSheet = bae_import::ReleaseEditSheet,
    from_core: fn,
    fields: {
        file_id,
        name,
        disc_number,
        disc_options,
        file_reference,
        audio_options: (each BridgeReleaseEditAudioOption),
    },
}

mirror_struct! {
    BridgeReleaseEditAudioOption = bae_import::ReleaseEditAudioOption,
    from_core: fn,
    fields: { file_id, name, bound, offer: (crate::types::BridgeSheetBindingOffer) },
}

impl BridgeReleaseEditTrackContext {
    fn from_core(context: bae_import::ReleaseEditTrackContext) -> Self {
        // The table's sides say which side a track is on, and a sheet's run
        // names the sheet that carves its tracks.
        let bae_import::ReleaseEditTrackContext {
            track_id,
            side: _,
            source,
            start_ms,
            duration_ms,
            listed_length_ms,
            position_text,
            sheet: _,
        } = context;
        Self {
            track_id,
            source: BridgeReleaseEditTrackSource::from_core(source),
            start_clock: crate::types::BridgeDurationClock::of_length(start_ms),
            duration_clock: crate::types::BridgeDurationClock::of_length(duration_ms),
            listed_length: listed_length_ms.map(crate::types::BridgeDurationClock::of_length),
            position_text,
        }
    }
}

mirror_struct! {
    BridgeReleaseEditTrackRow = bae_import::ReleaseEditTrackRow,
    from_core: fn,
    fields: {
        edit: (BridgeRawTrackEdit),
        context: (BridgeReleaseEditTrackContext),
        artists: (opt BridgeArtistField),
    },
}

mirror_enum! {
    BridgeReleaseEditTrackRun = bae_import::ReleaseEditTrackRun,
    from_core: fn,
    variants: {
        Track(row: (BridgeReleaseEditTrackRow)),
        Sheet {
            sheet: (BridgeReleaseEditSheet),
            source: (BridgeReleaseEditTrackSource),
            rows: (each BridgeReleaseEditTrackRow),
        },
    },
}

mirror_struct! {
    BridgeReleaseEditTrackSide = bae_import::ReleaseEditTrackSide,
    from_core: fn,
    fields: {
        side: (BridgeTrackSide),
        runs: (each BridgeReleaseEditTrackRun),
    },
}

/// `mirror_struct` takes no optional list, so this conversion names each
/// field by hand.
impl BridgeReleaseEditDraft {
    pub(crate) fn from_core(draft: bae_import::ReleaseEditDraft) -> Self {
        let bae_import::ReleaseEditDraft {
            cover,
            source_audio,
            sides,
            files,
            album,
            records,
            has_records,
            pressing_offers,
            album_artists,
            changed,
            files_to_read,
            invalid,
        } = draft;
        Self {
            cover: cover.map(BridgeImageRef::from_core),
            source_audio: source_audio.map(BridgeSourceAudioSummary::from_core),
            sides: sides.map(|sides| {
                sides.into_iter().map(BridgeReleaseEditTrackSide::from_core).collect()
            }),
            files: files.map(|files| files.into_iter().map(BridgeMappingFile::from_core).collect()),
            album: BridgeDraftAlbum::from_core(album),
            has_records,
            records: records.into_iter().map(BridgeReleaseRecord::from_core).collect(),
            pressing_offers: BridgePressingOffers::from_core(pressing_offers),
            album_artists: album_artists.map(BridgeArtistField::from_core),
            changed,
            files_to_read,
            invalid: invalid.map(BridgeValidationReason::from_core),
        }
    }
}
