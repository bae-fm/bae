//! A library of as many albums as the lists' window-cost measurements read,
//! each with an artist, one release, one medium and one track, and a work
//! the artist composed, written through the records' own row inserts.

use super::library::LibraryRows;
use crate::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseMedium, DbTrack, DbWork, DbWorkArtist,
    IdentityWriteError,
};
use bae_catalogs::catalog::Catalog;
use coven::CovenWrite;

/// A library row's id: a UUID naming its table by `kind` and its row by `i`,
/// as an SQL expression over `i`, for the other domains' rows that name it.
fn row_id_sql(kind: u32) -> String {
    format!("printf('%08x-{kind:04x}-4000-8000-%012x', i, i)")
}

/// Row `i`'s id in the table `kind` names, as [`row_id_sql`] writes it.
fn row_id(kind: u32, i: u64) -> String {
    format!("{i:08x}-{kind:04x}-4000-8000-{i:012x}")
}

/// Release `i`'s id, as an SQL expression over `i`.
pub fn release_id_sql() -> String {
    row_id_sql(3)
}

/// Track `i`'s id, as an SQL expression over `i`.
pub fn track_id_sql() -> String {
    row_id_sql(6)
}

/// Release `i`'s id.
pub fn release_id(i: u64) -> String {
    row_id(3, i)
}

/// File `j` of release `i`'s id.
pub fn file_id(i: u64, j: u64) -> String {
    format!("{i:08x}-0007-4000-8000-{j:012x}")
}

/// When row `i` was written.
pub fn created_at(i: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(
        1_600_000_000 + i64::try_from((i * 7919) % 50_000_000).unwrap(),
        0,
    )
    .unwrap()
}

/// Rows `start..end`: an artist, an album crediting it with one release, one
/// medium and one track, and a work the artist composed. Titles and names run
/// in an order of their own, unrelated to the ids', and every sort has ties
/// the ids break.
pub fn insert_on(sql: &CovenWrite<'_>, start: u64, end: u64) -> Result<(), IdentityWriteError> {
    let mut rows = LibraryRows::default();
    for i in start..end {
        let at = created_at(i);
        let (artist, album, release, track) =
            (row_id(1, i), row_id(2, i), release_id(i), row_id(6, i));
        let musicbrainz_work = row_id(8, i);
        let work = crate::identity::work_id(&musicbrainz_work);
        rows.artists.push(DbArtist {
            id: artist.clone(),
            name: format!("Artist {:06}", (i * 48271) % 1_000_003),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: at,
        });
        rows.albums.push(DbAlbum {
            id: album.clone(),
            title: format!("Album {:05}", (i * 16807) % 99_991),
            year: (i % 17 != 0).then(|| 1950 + i32::try_from((i * 31) % 70).unwrap()),
            is_compilation: false,
            created_at: at,
        });
        rows.album_artists.push(DbAlbumArtist::new(&album, &artist, 0, at));
        rows.releases.push(DbRelease { created_at: at, ..DbRelease::new_test(&album, &release) });
        rows.mediums.push(DbReleaseMedium::new(
            &release,
            std::num::NonZeroU16::MIN,
            bae_types::mediums::ReleaseMedium::one_sided(None),
            at,
        ));
        rows.tracks.push(DbTrack {
            created_at: at,
            ..DbTrack::new_test(&release, &track, &format!("Track {i:06}"), 1)
        });
        rows.works.push(DbWork {
            id: work.clone(),
            title: format!("Work {i:06}"),
            disambiguation: None,
            work_type: None,
            musicbrainz_work_id: musicbrainz_work,
            created_at: at,
        });
        rows.work_artists.push(DbWorkArtist::new(
            &work,
            &artist,
            0,
            Catalog::MusicBrainz,
            bae_types::credit_role::CreditedAs::of(
                bae_types::credit_role::CreditRoleKind::Composer,
            ),
            None,
            at,
        ));
    }
    let labels = rows.label_parts(&coven::UuidIds, created_at(start));
    rows.write_on(sql, &labels)
}
