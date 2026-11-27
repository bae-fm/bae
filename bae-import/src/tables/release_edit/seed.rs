//! A library release as its editor reads it, and the seed an editor opens
//! on: the release's form, tracks and album as the library holds them.

use super::*;
use crate::{DraftAlbum, RawReleaseEdit, RawTrackEdit, TrackArtistAssignments};

/// A release as the editor reads it: its detail, its album's title, year and
/// artists, and its tracks' audio in release order.
pub struct ReleaseEditContext {
    pub detail: bae_records::DbReleaseDetail,
    pub album_title: String,
    pub album_year: Option<i32>,
    pub album_artists: Vec<bae_records::credits::LibraryCredit>,
    pub tracks: Vec<bae_records::DbReleaseEditTrack>,
}

/// The rows that make up a release's edit seed, as `sql` holds them: its
/// detail with its album's title, year and artists, and its tracks' audio in
/// release order. `None` when the release is gone.
pub(crate) fn release_edit_rows_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Option<ReleaseEditRows>, DbError> {
    let Some(release) = bae_records::records::release_editor_rows_on(sql, release_id)? else {
        return Ok(None);
    };
    let tracks = bae_records::records::release_edit_tracks_on(sql, release_id)?;
    Ok(Some(ReleaseEditRows { release, tracks }))
}

/// [`release_edit_rows_on`]'s rows.
pub(crate) struct ReleaseEditRows {
    release: bae_records::records::ReleaseEditorRows,
    tracks: Vec<bae_records::DbReleaseEditTrack>,
}

impl ReleaseEditRows {
    /// The rows as the editor reads them.
    pub(crate) fn processed(self) -> Result<ReleaseEditContext, DbError> {
        let Self { release, tracks } = self;
        let bae_records::records::ReleaseEditorFacts {
            detail,
            album_title,
            album_year,
            album_artists,
        } = release.decoded()?;
        Ok(ReleaseEditContext { detail, album_title, album_year, album_artists, tracks })
    }
}

/// The editor's seed for a release as `context` holds it, under the album
/// the release is in: the raw editor form that holds the album title and
/// artists, the release's pressing fields and each track's title and
/// artists. A track with no artist rows of its own seeds as taking the album's
/// artists; the album artists seed the album artist field.
pub(crate) fn release_edit_seed(
    context: ReleaseEditContext,
) -> Result<ReleaseEditSeed, ReleaseEditLoadError> {
    let ReleaseEditContext { detail, album_title, album_year, album_artists, tracks } = context;
    let mut sheet_places = std::collections::BTreeMap::new();
    let tracks = tracks.into_iter().inspect(|track| {
        if let Some(sheet) = &track.sheet {
            sheet_places.insert(sheet.file_id.clone(), sheet.place.side);
        }
    });
    let display = crate::ReleaseEditDisplayContext::from_raw(&detail.files, tracks)?;
    let release = detail.release;
    let artist_line = bae_records::credits::library_line(&album_artists);
    let edit = RawReleaseEdit {
        album_title: album_title.clone(),
        album_artist_assignments: album_artists
            .into_iter()
            .map(crate::Credited::of_library)
            .collect(),
        album_year: album_year.map(|year| year.to_string()).unwrap_or_default(),
        pressing: crate::RawPressingEdit::from_pressing(&release.pressing),
        tracks: detail
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .map(|entry| {
                let artists: Vec<crate::Credited> =
                    entry.artists.iter().cloned().map(crate::Credited::of_library).collect();
                RawTrackEdit {
                    id: entry.id.clone(),
                    title: entry.title.clone(),
                    artist_assignments: if artists.is_empty() {
                        TrackArtistAssignments::AlbumArtists
                    } else {
                        TrackArtistAssignments::Explicit(artists)
                    },
                    // An edit never binds a library track's audio again.
                    file: None,
                }
            })
            .collect(),
    };
    Ok(ReleaseEditSeed {
        edit,
        display,
        sheet_places,
        album: DraftAlbum::Named {
            album_id: release.album_id,
            line: crate::AlbumLine::of(album_title, artist_line),
        },
    })
}
