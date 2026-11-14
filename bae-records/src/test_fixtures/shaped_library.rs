//! A library with the shape of a real one, grown album by album: box sets and
//! vinyl sides, compilations, several releases of an album, a few artists
//! holding most albums, classical works shared across releases, role credits,
//! other printed names and artist merges. Every row is a function of its
//! album's number, so a library of `n` albums holds the rows of every smaller
//! one, written through the records' own row inserts.
//!
//! Album `i` holds 12 tracks; every 50th a box set of 120 tracks on 6 discs,
//! every 500th one of 600 tracks on 30 discs; every 10th is on vinyl, its
//! sides A to D. Album artists: one (85%), two (10%), or a compilation by
//! "Various Artists" (5%) whose tracks each credit their own artist. The
//! artists number a quarter of the albums, drawn so the top one holds about
//! albums / 40. One album in ten has a second release and one in fifty a
//! third. Three in ten releases carry 2-4 role credits on each track and 1-3
//! on the release. One artist in twenty has another printed name; one album
//! in four hundred ends with an artist merged into an earlier one. 15% of the
//! albums are classical: their tracks perform works, a third of which are
//! parts of another work, composed by a pool of composers drawn so the first
//! holds thousands.

use super::library::LibraryRows;
use crate::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbReleaseMedium, DbTrack,
    DbTrackArtist, DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart,
    IdentityWriteError,
};
use bae_catalogs::catalog::Catalog;
use bae_types::credit_role::{CreditRoleKind, CreditedAs};
use bae_types::mediums::ReleaseMedium;
use bae_types::pressing::Medium;
use chrono::{DateTime, Utc};
use coven::CovenWrite;
use std::num::{NonZeroU16, NonZeroU32, NonZeroU8};

/// How many albums one write holds.
const CHUNK: u64 = 250;

/// Album `i`'s number of tracks on its first release.
pub fn track_count(i: u64) -> u64 {
    match i {
        _ if i % 500 == 250 => 600,
        _ if i % 50 == 25 => 120,
        _ => 12,
    }
}

/// How many discs album `i`'s first release has.
pub fn disc_count(i: u64) -> u64 {
    match track_count(i) {
        600 => 30,
        120 => 6,
        _ => 1,
    }
}

/// Whether album `i`'s first release is on vinyl, with sides A to D.
pub fn is_vinyl(i: u64) -> bool {
    i % 10 == 3 && track_count(i) == 12
}

/// Whether album `i` is a compilation.
pub fn is_compilation(i: u64) -> bool {
    mix(i, 1) % 100 >= 95
}

/// Whether album `i` is classical: its tracks perform works.
pub fn is_classical(i: u64) -> bool {
    mix(i, 2) % 100 < 15
}

/// How many releases album `i` has.
pub fn release_count(i: u64) -> u64 {
    match mix(i, 3) % 100 {
        0..=1 => 3,
        2..=11 => 2,
        _ => 1,
    }
}

/// Whether album `i` ends with an artist merged into an earlier one.
pub fn has_merge(i: u64) -> bool {
    i % 400 == 399
}

fn mix(i: u64, salt: u64) -> u64 {
    let mut x = i.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(salt.wrapping_mul(0xBF58_476D));
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A draw from `0..pool` that favours the low numbers: the more, the higher
/// `skew`.
fn skewed(key: u64, salt: u64, pool: u64, skew: i32) -> u64 {
    let unit = (mix(key, salt) >> 11) as f64 / (1u64 << 53) as f64;
    ((pool as f64 * unit.powi(skew)) as u64).min(pool - 1)
}

/// The artists drawn from when album `i` is written, a quarter of the
/// albums so far.
fn artist_pool(i: u64) -> u64 {
    (i + 1).div_ceil(4)
}

/// The works drawn from when album `i` is written.
fn work_pool(i: u64) -> u64 {
    (i + 1).div_ceil(3)
}

fn composer_of(work: u64) -> u64 {
    skewed(work, 11, (work * 3 / 50).max(1), 5)
}

/// A row's id: a UUID naming its table by `kind`, its owner by `major` and
/// the row by `minor`.
fn id(kind: u32, major: u64, minor: u64) -> String {
    format!("{major:08x}-{kind:04x}-4000-8000-{minor:012x}")
}

pub fn artist_id(p: u64) -> String {
    id(1, p, 0)
}

pub fn album_id(i: u64) -> String {
    id(2, i, 0)
}

/// The id of release `r` of album `i`.
pub fn release_id(i: u64, r: u64) -> String {
    id(3, i, r)
}

/// The id of track `k` (from 0) of release `r` of album `i`.
pub fn track_id(i: u64, r: u64, k: u64) -> String {
    id(6, i, r * 1000 + k)
}

/// The id of album `i`'s first release, as an SQL expression over `i`, for
/// the other domains' rows that name it.
pub fn first_release_id_sql() -> String {
    "printf('%08x-0003-4000-8000-%012x', i, 0)".to_string()
}

/// The id of the first track of album `i`'s first release, as an SQL
/// expression over `i`.
pub fn first_track_id_sql() -> String {
    "printf('%08x-0006-4000-8000-%012x', i, 0)".to_string()
}

pub fn work_id(w: u64) -> String {
    crate::identity::work_id(&id(8, w, 0))
}

/// Album `i`'s title and year, in an order unrelated to the numbers.
fn album(i: u64, at: DateTime<Utc>) -> DbAlbum {
    DbAlbum {
        id: album_id(i),
        title: format!("Album {:05}", (i * 16807) % 99_991),
        year: (!i.is_multiple_of(17)).then(|| 1950 + i32::try_from((i * 31) % 70).unwrap()),
        is_compilation: is_compilation(i),
        created_at: at,
    }
}

fn at(i: u64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_600_000_000 + i64::try_from((i * 7919) % 50_000_000).unwrap(), 0)
        .unwrap()
}

fn medium_row(
    release: &str,
    disc: u64,
    medium: ReleaseMedium,
    created: DateTime<Utc>,
) -> DbReleaseMedium {
    let named =
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, format!("{release}/{disc}").as_bytes());
    DbReleaseMedium {
        id: uuid::Builder::from_random_bytes(*named.as_bytes()).into_uuid().to_string(),
        release_id: release.to_string(),
        position: NonZeroU16::new(u16::try_from(disc + 1).unwrap()).unwrap(),
        medium,
        created_at: created,
    }
}

fn medium_id(release: &str, disc: u64) -> String {
    medium_row(release, disc, ReleaseMedium::one_sided(None), Utc::now()).id
}

/// Rows for albums `start..end`.
pub fn insert_on(sql: &CovenWrite<'_>, start: u64, end: u64) -> Result<(), IdentityWriteError> {
    let mut from = start;
    while from < end {
        let to = (from + CHUNK).min(end);
        insert_chunk(sql, from, to)?;
        from = to;
    }
    Ok(())
}

fn insert_chunk(sql: &CovenWrite<'_>, start: u64, end: u64) -> Result<(), IdentityWriteError> {
    let mut rows = LibraryRows::default();
    let mut merges = Vec::new();
    let mut other_names = Vec::new();
    for i in start..end {
        let created = at(i);
        if i.is_multiple_of(4) {
            let p = i / 4;
            rows.artists.push(DbArtist {
                id: artist_id(p),
                name: if p == 0 {
                    "Various Artists".to_string()
                } else {
                    format!("Artist {:06}", (p * 48271) % 1_000_003)
                },
                sort_name: None,
                discogs_artist_id: None,
                musicbrainz_artist_id: None,
                created_at: created,
            });
            if p % 20 == 7 {
                other_names.push((p, format!("A. Artist {p:06}")));
            }
        }
        if i.is_multiple_of(3) {
            let w = i / 3;
            let musicbrainz_work = id(8, w, 0);
            let work = work_id(w);
            rows.works.push(DbWork {
                id: work.clone(),
                title: format!("Work {w:06}"),
                disambiguation: None,
                work_type: None,
                musicbrainz_work_id: musicbrainz_work,
                created_at: created,
            });
            rows.work_artists.push(DbWorkArtist::new(
                &work,
                &artist_id(composer_of(w)),
                0,
                Catalog::MusicBrainz,
                CreditedAs::of(CreditRoleKind::Composer),
                None,
                created,
            ));
            if w % 3 == 1 && w >= 3 {
                rows.work_parts.push(DbWorkPart::new(
                    &work_id(w - 1),
                    &work,
                    0,
                    Catalog::MusicBrainz,
                    created,
                ));
            }
        }
        rows.albums.push(album(i, created));
        let pool = artist_pool(i);
        if is_compilation(i) {
            // "Various Artists" is an artist of its own, the first one.
            rows.album_artists.push(DbAlbumArtist::new(
                &coven::UuidIds,
                &album_id(i),
                &artist_id(0),
                0,
                created,
            ));
        } else {
            let first = skewed(i, 4, pool, 2);
            rows.album_artists.push(DbAlbumArtist::new(
                &coven::UuidIds,
                &album_id(i),
                &artist_id(first),
                0,
                created,
            ));
            if mix(i, 5) % 100 >= 85 {
                let second = skewed(i, 6, pool, 2);
                if second != first {
                    rows.album_artists.push(DbAlbumArtist::new(
                        &coven::UuidIds,
                        &album_id(i),
                        &artist_id(second),
                        1,
                        created,
                    ));
                }
            }
        }
        for r in 0..release_count(i) {
            let release = release_id(i, r);
            rows.releases.push(DbRelease {
                created_at: created,
                ..DbRelease::new_test(&album_id(i), &release)
            });
            let tracks = if r == 0 { track_count(i) } else { 12 };
            let discs = if r == 0 { disc_count(i) } else { 1 };
            let vinyl = r == 0 && is_vinyl(i);
            for disc in 0..discs {
                let medium = match (vinyl, discs) {
                    (true, _) => ReleaseMedium {
                        carrier: Some(Medium::Vinyl),
                        sides: NonZeroU8::new(4).unwrap(),
                    },
                    _ => ReleaseMedium::one_sided(Some(Medium::Cd)),
                };
                rows.mediums.push(medium_row(&release, disc, medium, created));
            }
            let per_disc = tracks / discs;
            for k in 0..tracks {
                let disc = k / per_disc;
                let on_disc = k % per_disc;
                let (side, number) = if vinyl {
                    (1 + on_disc * 4 / per_disc, 1 + on_disc % (per_disc / 4))
                } else {
                    (1, 1 + on_disc)
                };
                let track = track_id(i, r, k);
                rows.tracks.push(DbTrack {
                    id: track.clone(),
                    release_id: release.clone(),
                    title: format!("Track {i:06}-{r}-{k:03}"),
                    medium_id: medium_id(&release, disc),
                    side: NonZeroU8::new(u8::try_from(side).unwrap()).unwrap(),
                    track_number: NonZeroU32::new(u32::try_from(number).unwrap()).unwrap(),
                    duration_ms: 120_000 + (mix(i * 1000 + k, 7) % 240_000),
                    created_at: created,
                });
                if is_compilation(i) {
                    let own = skewed(i * 1000 + k, 8, pool, 2);
                    rows.track_artists.push(DbTrackArtist::new(
                        &coven::UuidIds,
                        &track,
                        &artist_id(own),
                        0,
                        created,
                    ));
                }
                if is_classical(i) && r == 0 {
                    let w = skewed(i * 1000 + k, 9, work_pool(i), 2);
                    rows.track_works.push(DbTrackWork::new(
                        &track,
                        &work_id(w),
                        0,
                        Catalog::MusicBrainz,
                        id(9, i, k),
                        created,
                    ));
                }
            }
            if mix(i * 4 + r, 10) % 100 < 30 {
                let kinds =
                    [CreditRoleKind::Producer, CreditRoleKind::Engineer, CreditRoleKind::Mixer];
                for n in 0..1 + mix(i * 4 + r, 12) % 3 {
                    rows.release_artist_roles.push(DbReleaseArtistRole::new(
                        &release,
                        &artist_id(skewed(i * 4 + r, 13 + n, pool, 2)),
                        i32::try_from(n).unwrap(),
                        Catalog::MusicBrainz,
                        CreditedAs::of(kinds[n as usize]),
                        None,
                        id(10, i * 4 + r, n),
                        created,
                    ));
                }
                for k in 0..tracks {
                    for n in 0..2 + mix(i * 1000 + k, 14) % 3 {
                        rows.track_artist_roles.push(DbTrackArtistRole::new(
                            &track_id(i, r, k),
                            &artist_id(skewed(i * 1000 + k, 20 + n, pool, 2)),
                            i32::try_from(n).unwrap(),
                            Catalog::MusicBrainz,
                            CreditedAs::of(kinds[n as usize % 3]),
                            None,
                            id(11, i * 4 + r, k * 8 + n),
                            created,
                        ));
                    }
                }
            }
        }
        if has_merge(i) {
            merges.push(i / 4);
        }
    }
    let labels = rows.pressing_parts(&coven::UuidIds, at(start));
    rows.write_on(sql, &labels)?;
    for (artist, name) in other_names {
        crate::records::artist_credit_names::add_credit_name_on(
            sql,
            &artist_id(artist),
            &name,
            &at(start).to_rfc3339(),
        )?;
    }
    for p in merges {
        let (absorbed, survivor) = (artist_id(p), artist_id(p / 2));
        crate::records::identity_merges::merge_artist_on(
            sql,
            &crate::records::identity_merges::ArtistMerge {
                survivor: &survivor,
                absorbed: &absorbed,
                discogs_artist_id: None,
                musicbrainz_artist_id: None,
                sort_name: None,
                at: &at(start).to_rfc3339(),
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "shaped_library_tests.rs"]
mod tests;
