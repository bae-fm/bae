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
    BridgeCreditResolution = bae_records::CreditResolution,
    from_core: fn,
    into_core: fn,
    variants: {
        Library { artist: (BridgeExistingArtist) },
        New,
        Ambiguous { artists: (each BridgeExistingArtist) },
    },
}

mirror_struct! {
    BridgeResolvedCredit = bae_records::ResolvedCredit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { credit: (BridgeArtistCredit), resolution: (BridgeCreditResolution) },
}

mirror_enum! {
    BridgeArtistStanding = bae_import::ArtistStanding,
    from_core: pub(crate) fn,
    variants: {
        Library,
        New,
        Choose { choices: (each BridgeExistingArtist) },
    },
}

mirror_enum! {
    BridgeArtistsStanding = bae_import::ArtistsStanding,
    from_core: pub(crate) fn,
    variants: {
        Library,
        New,
        SomeNew { count },
        Choose { choices },
        SomeToChoose { count },
    },
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

mirror_struct! {
    BridgeRawPressingEdit = bae_import::RawPressingEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        year,
        labels: (each BridgeRawLabelEdit),
        barcodes,
        facts: (BridgePressingFacts),
    },
}

/// The editor shows and sends a label row's name and number alone: the ids a
/// catalog gave the row stay in the core, which gives them back to a row that
/// keeps the name they came with.
impl BridgeRawLabelEdit {
    pub(crate) fn from_core(row: bae_import::RawLabelEdit) -> Self {
        Self { name: row.name, catalog_number: row.catalog_number }
    }

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

impl BridgeReleaseEditTrackContext {
    fn from_core(context: bae_import::ReleaseEditTrackContext) -> Self {
        let bae_import::ReleaseEditTrackContext { track_id, source, duration_ms, position_text } =
            context;
        Self {
            track_id,
            source: BridgeReleaseEditTrackSource::from_core(source),
            duration_clock: crate::types::BridgeDurationClock::of_length(duration_ms),
            position_text,
        }
    }
}

mirror_enum! {
    BridgeReleaseEditTrackRun = bae_import::ReleaseEditTrackRun,
    from_core: fn,
    variants: {
        Track(track: (BridgeReleaseEditTrackContext)),
        Sheet {
            source: (BridgeReleaseEditTrackSource),
            tracks: (each BridgeReleaseEditTrackContext),
        },
    },
}

/// Not a `mirror_struct`: `header_key` is derived from the side rather than
/// carried by core, so a side reads a field instead of asking.
impl BridgeReleaseEditTrackSide {
    fn from_core(side: bae_import::ReleaseEditTrackSide) -> Self {
        let bae_import::ReleaseEditTrackSide { side, runs } = side;
        let side = BridgeTrackSide::from_core(side);
        Self {
            header_key: side.header_key().map(str::to_string),
            side,
            runs: runs.into_iter().map(BridgeReleaseEditTrackRun::from_core).collect(),
        }
    }
}

mirror_struct! {
    BridgeReleaseEditDisplayContext = bae_import::ReleaseEditDisplayContext,
    from_core: fn,
    fields: {
        source_audio: (opt BridgeSourceAudioSummary),
        sides: (each BridgeReleaseEditTrackSide),
        files: (each BridgeMappingFile),
    },
}

mirror_struct! {
    BridgeReleaseEditDraft = bae_import::ReleaseEditDraft,
    from_core: pub(crate) fn,
    fields: {
        edit: (BridgeRawReleaseEdit),
        cover: (opt BridgeImageRef),
        display: (BridgeReleaseEditDisplayContext),
        album: (BridgeDraftAlbum),
        artist_resolutions: (each BridgeResolvedCredit),
        changed,
        invalid: (opt BridgeValidationReason),
    },
}
