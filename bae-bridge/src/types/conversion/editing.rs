use super::super::*;

mirror_struct! {
    BridgeNewArtistSeed = bae_core::import::NewArtistSeed,
    from_core: fn,
    into_core: fn,
    fields: { name, sort_name, musicbrainz_artist_id, discogs_artist_id },
}

mirror_enum! {
    BridgeArtistAssignment = bae_core::import::ArtistAssignment,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Existing { artist: (BridgeExistingArtist) },
        New { seed: (BridgeNewArtistSeed) },
    },
}

mirror_enum! {
    BridgeTrackArtistAssignments = bae_core::import::TrackArtistAssignments,
    from_core: pub(super) fn,
    into_core: pub(crate) fn,
    variants: {
        AlbumArtists,
        Explicit(assignments: (each BridgeArtistAssignment)),
    },
}

mirror_struct! {
    BridgeTrackUserEdit = bae_core::import::TrackUserEdit,
    from_core: pub(crate) fn,
    into_core: fn,
    fields: {
        title,
        side,
        track_number,
        artist_assignments: (BridgeTrackArtistAssignments),
        file: (opt BridgeAudioFile),
    },
}

mirror_struct! {
    BridgeReleaseUserEdit = bae_core::import::ReleaseUserEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        album_title,
        album_artist_assignments: (each BridgeArtistAssignment),
        album_year,
        pressing: (BridgePressingEdit),
        tracks: (each BridgeTrackUserEdit),
    },
}

mirror_struct! {
    BridgeRawPressingEdit = bae_core::import::RawPressingEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { year, format, label, catalog_number, country, barcode },
}

mirror_struct! {
    BridgeRawTrackEdit = bae_core::import::RawTrackEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        id,
        title,
        artist_assignments: (BridgeTrackArtistAssignments),
        side,
        track_number,
        file: (opt BridgeAudioFile),
    },
}

mirror_struct! {
    BridgeRawReleaseEdit = bae_core::import::RawReleaseEdit,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        album_title,
        album_artist_assignments: (each BridgeArtistAssignment),
        album_year,
        pressing: (BridgeRawPressingEdit),
        tracks: (each BridgeRawTrackEdit),
    },
}

mirror_struct! {
    BridgeReleaseEditTrackSource = bae_core::album_detail::ReleaseEditTrackSource,
    from_core: fn,
    fields: { file_id, name, layout: (BridgeSourceAudioLayout) },
}

/// Not a `mirror_struct`: `side_header_key` is derived from the side rather
/// than carried by core, so a track row reads a field instead of asking.
impl BridgeReleaseEditTrackContext {
    fn from_core(context: bae_core::album_detail::ReleaseEditTrackContext) -> Self {
        let bae_core::album_detail::ReleaseEditTrackContext {
            track_id,
            sources,
            duration_ms,
            side,
        } = context;
        let side = BridgeTrackSide::from_core(side);
        let side_header_key = side.header_key().map(str::to_string);
        Self {
            track_id,
            sources: sources
                .into_iter()
                .map(BridgeReleaseEditTrackSource::from_core)
                .collect(),
            duration_ms,
            side,
            side_header_key,
        }
    }
}

mirror_struct! {
    BridgeReleaseEditDisplayContext = bae_core::album_detail::ReleaseEditDisplayContext,
    from_core: fn,
    fields: {
        source_audio: (opt BridgeSourceAudioSummary),
        tracks: (each BridgeReleaseEditTrackContext),
    },
}

mirror_struct! {
    BridgeReleaseEditSeed = bae_core::import::ReleaseEditSeed,
    from_core: pub(crate) fn,
    fields: {
        edit: (BridgeRawReleaseEdit),
        can_reset_to_source,
        cover: (opt BridgeImageRef),
        display: (BridgeReleaseEditDisplayContext),
    },
}
