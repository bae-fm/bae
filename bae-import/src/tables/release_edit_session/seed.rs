//! A library release as its editor reads it, and the seed a session opens
//! on: the release's form, tracks and album as the library holds them.

use super::*;

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
    let Some((detail, (album_title, album_year, album_artists))) =
        bae_records::records::release_detail_rows_on(
            sql,
            release_id,
            bae_records::records::NO_STARS,
            &bae_records::records::read::album_of_release_sql(),
            bae_records::records::read::album_of_release_columns,
        )?
    else {
        return Ok(None);
    };
    let tracks = bae_records::records::release_edit_tracks_on(
        sql,
        "?1",
        bae_records::records::MediumsSource::Release,
        [release_id],
    )?;
    Ok(Some(ReleaseEditRows { detail, album_title, album_year, album_artists, tracks }))
}

/// [`release_edit_rows_on`]'s rows.
pub(crate) struct ReleaseEditRows {
    detail: bae_records::records::DbReleaseDetailRows,
    album_title: String,
    album_year: Option<i32>,
    album_artists: Vec<bae_records::credits::LibraryCredit>,
    tracks: Vec<bae_records::DbReleaseEditTrack>,
}

impl ReleaseEditRows {
    /// The rows as the editor reads them.
    pub(crate) fn processed(self) -> ReleaseEditContext {
        let Self { detail, album_title, album_year, album_artists, tracks } = self;
        ReleaseEditContext {
            detail: detail.process(),
            album_title,
            album_year,
            album_artists,
            tracks,
        }
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
        album: DraftAlbum::Named { album_id: release.album_id, title: album_title, artist_line },
    })
}

/// The seed of the release `release_id` as the write `sql` finds it, which
/// the write stores as what a session opened on or checks a save against.
pub(super) fn seed_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<Result<ReleaseEditSeed, ReleaseEditSeedRefusal>, DbError> {
    Ok(match release_edit_rows_on(sql, release_id)? {
        None => Err(ReleaseEditSeedRefusal::ReleaseMissing(release_id.to_string())),
        Some(rows) => release_edit_seed(rows.processed()).map_err(ReleaseEditSeedRefusal::from),
    })
}
