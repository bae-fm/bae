use super::super::*;

#[cfg(feature = "desktop")]
impl BridgeArtistAssignment {
    pub(crate) fn from_core(assignment: bae_core::import::ArtistAssignment) -> Self {
        match assignment {
            bae_core::import::ArtistAssignment::Existing { artist } => Self::Existing {
                artist: BridgeExistingArtist::from_core(artist),
            },
            bae_core::import::ArtistAssignment::New { seed } => Self::New {
                seed: BridgeNewArtistSeed {
                    name: seed.name,
                    sort_name: seed.sort_name,
                    musicbrainz_artist_id: seed.musicbrainz_artist_id,
                    discogs_artist_id: seed.discogs_artist_id,
                },
            },
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::ArtistAssignment {
        match self {
            Self::Existing { artist } => bae_core::import::ArtistAssignment::Existing {
                artist: artist.into_core(),
            },
            Self::New { seed } => bae_core::import::ArtistAssignment::New {
                seed: bae_core::import::NewArtistSeed {
                    name: seed.name,
                    sort_name: seed.sort_name,
                    musicbrainz_artist_id: seed.musicbrainz_artist_id,
                    discogs_artist_id: seed.discogs_artist_id,
                },
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeTrackArtistAssignments {
    pub(super) fn from_core(assignments: bae_core::import::TrackArtistAssignments) -> Self {
        match assignments {
            bae_core::import::TrackArtistAssignments::AlbumArtists => Self::AlbumArtists,
            bae_core::import::TrackArtistAssignments::Explicit(assignments) => Self::Explicit {
                assignments: assignments
                    .into_iter()
                    .map(BridgeArtistAssignment::from_core)
                    .collect(),
            },
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::TrackArtistAssignments {
        match self {
            Self::AlbumArtists => bae_core::import::TrackArtistAssignments::AlbumArtists,
            Self::Explicit { assignments } => bae_core::import::TrackArtistAssignments::Explicit(
                assignments
                    .into_iter()
                    .map(BridgeArtistAssignment::into_core)
                    .collect(),
            ),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeTrackUserEdit {
    fn from_core(t: bae_core::import::TrackUserEdit) -> Self {
        let bae_core::import::TrackUserEdit {
            title,
            side,
            track_number,
            artist_assignments,
            file,
        } = t;
        Self {
            title,
            side,
            track_number,
            artist_assignments: BridgeTrackArtistAssignments::from_core(artist_assignments),
            file: file.map(BridgeAudioFile::from_core),
        }
    }

    fn into_core(self) -> bae_core::import::TrackUserEdit {
        let BridgeTrackUserEdit {
            title,
            side,
            track_number,
            artist_assignments,
            file,
        } = self;
        bae_core::import::TrackUserEdit {
            title,
            side,
            track_number,
            artist_assignments: artist_assignments.into_core(),
            file: file.map(BridgeAudioFile::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseUserEdit {
    pub(crate) fn from_core(e: bae_core::import::ReleaseUserEdit) -> Self {
        let bae_core::import::ReleaseUserEdit {
            album_title,
            album_artist_assignments,
            pressing,
            tracks,
        } = e;
        BridgeReleaseUserEdit {
            album_title,
            album_artist_assignments: album_artist_assignments
                .into_iter()
                .map(BridgeArtistAssignment::from_core)
                .collect(),
            pressing: BridgePressingEdit::from_core(pressing),
            tracks: tracks
                .into_iter()
                .map(BridgeTrackUserEdit::from_core)
                .collect(),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::ReleaseUserEdit {
        let BridgeReleaseUserEdit {
            album_title,
            album_artist_assignments,
            pressing,
            tracks,
        } = self;
        bae_core::import::ReleaseUserEdit {
            album_title,
            album_artist_assignments: album_artist_assignments
                .into_iter()
                .map(BridgeArtistAssignment::into_core)
                .collect(),
            pressing: pressing.into_core(),
            tracks: tracks
                .into_iter()
                .map(BridgeTrackUserEdit::into_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawPressingEdit {
    pub(crate) fn from_core(p: bae_core::import::RawPressingEdit) -> Self {
        let bae_core::import::RawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = p;
        Self {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::RawPressingEdit {
        let BridgeRawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = self;
        bae_core::import::RawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawTrackEdit {
    pub(crate) fn from_core(t: bae_core::import::RawTrackEdit) -> Self {
        let bae_core::import::RawTrackEdit {
            id,
            title,
            artist_assignments,
            side,
            track_number,
            file,
        } = t;
        Self {
            id,
            title,
            artist_assignments: BridgeTrackArtistAssignments::from_core(artist_assignments),
            side,
            track_number,
            file: file.map(BridgeAudioFile::from_core),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::RawTrackEdit {
        let BridgeRawTrackEdit {
            id,
            title,
            artist_assignments,
            side,
            track_number,
            file,
        } = self;
        bae_core::import::RawTrackEdit {
            id,
            title,
            artist_assignments: artist_assignments.into_core(),
            side,
            track_number,
            file: file.map(BridgeAudioFile::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawReleaseEdit {
    pub(crate) fn from_core(e: bae_core::import::RawReleaseEdit) -> Self {
        let bae_core::import::RawReleaseEdit {
            album_title,
            album_artist_assignments,
            pressing,
            tracks,
        } = e;
        BridgeRawReleaseEdit {
            album_title,
            album_artist_assignments: album_artist_assignments
                .into_iter()
                .map(BridgeArtistAssignment::from_core)
                .collect(),
            pressing: BridgeRawPressingEdit::from_core(pressing),
            tracks: tracks
                .into_iter()
                .map(BridgeRawTrackEdit::from_core)
                .collect(),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::RawReleaseEdit {
        let BridgeRawReleaseEdit {
            album_title,
            album_artist_assignments,
            pressing,
            tracks,
        } = self;
        bae_core::import::RawReleaseEdit {
            album_title,
            album_artist_assignments: album_artist_assignments
                .into_iter()
                .map(BridgeArtistAssignment::into_core)
                .collect(),
            pressing: pressing.into_core(),
            tracks: tracks
                .into_iter()
                .map(BridgeRawTrackEdit::into_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseEditSeed {
    pub(crate) fn from_core(seed: bae_core::import::ReleaseEditSeed) -> Self {
        let bae_core::import::ReleaseEditSeed {
            edit,
            can_reset_to_source,
        } = seed;
        Self {
            edit: BridgeRawReleaseEdit::from_core(edit),
            can_reset_to_source,
        }
    }
}
