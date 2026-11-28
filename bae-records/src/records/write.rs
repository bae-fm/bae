use super::*;
use bae_store::WriteStatements as _;
use bae_util::timestamp::StoredTimestamp as _;

// The `album_order` that puts a release last among those of the album `album`.
coven::fragment!(
    NextAlbumOrder(album: String) { next: i64 },
    r#"
    SELECT COALESCE(MAX(r.album_order) + 1, 0) AS next
    FROM releases AS r
    WHERE r.album_id = :album
"#
);

coven::write!(
    InsertArtist(
        id: &'p str,
        name: &'p str,
        name_key: String,
        sort_name: Option<&'p str>,
        discogs_artist_id: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        created_at: String,
    ),
    r#"
    INSERT INTO artists (
        id, name, name_key, sort_name, discogs_artist_id,
        musicbrainz_artist_id, created_at
    ) VALUES (
        :id, :name, :name_key, :sort_name, :discogs_artist_id,
        :musicbrainz_artist_id, :created_at
    )
"#
);

pub(super) fn insert_artist_row(conn: &WriteContext<'_>, artist: &DbArtist) -> Result<(), DbError> {
    conn.run(&InsertArtistParams {
        id: &artist.id,
        name: &artist.name,
        name_key: bae_types::text_match::artist_name_key(&artist.name),
        sort_name: artist.sort_name.as_deref(),
        discogs_artist_id: artist.discogs_artist_id.as_deref(),
        musicbrainz_artist_id: artist.musicbrainz_artist_id.as_deref(),
        created_at: artist.created_at.to_rfc3339(),
    })?;
    Ok(())
}

// Give the artist `id` each of these that it has none of.
coven::write!(
    FillArtistIds(
        id: &'p str,
        discogs_artist_id: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        sort_name: Option<&'p str>,
    ),
    r#"
    UPDATE artists SET
        discogs_artist_id = COALESCE(discogs_artist_id, :discogs_artist_id),
        musicbrainz_artist_id = COALESCE(musicbrainz_artist_id, :musicbrainz_artist_id),
        sort_name = COALESCE(sort_name, :sort_name)
    WHERE id = :id
"#
);

pub(super) fn update_artist_external_ids_row(
    conn: &WriteContext<'_>,
    id: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
    sort_name: Option<&str>,
) -> Result<usize, DbError> {
    Ok(conn.run(&FillArtistIdsParams {
        id,
        discogs_artist_id,
        musicbrainz_artist_id,
        sort_name,
    })?)
}

// Callers: a records change that files a release under a new album, the test
// writes, and the debug build's seeded albums.
#[cfg(any(test, feature = "test-utils", desktop))]
coven::write!(
    InsertAlbum(
        id: &'p str,
        title: &'p str,
        year: Option<i32>,
        is_compilation: bool,
        created_at: String,
    ),
    r#"
    INSERT INTO albums (id, title, year, is_compilation, created_at)
    VALUES (:id, :title, :year, :is_compilation, :created_at)
"#
);

#[cfg(any(test, feature = "test-utils", desktop))]
pub(super) fn insert_album_row(conn: &WriteContext<'_>, album: &DbAlbum) -> Result<(), DbError> {
    conn.run(&InsertAlbumParams {
        id: &album.id,
        title: &album.title,
        year: album.year,
        is_compilation: album.is_compilation,
        created_at: album.created_at.to_rfc3339(),
    })?;
    Ok(())
}

#[cfg(desktop)]
coven::write!(
    UpsertAlbum(
        id: &'p str,
        title: &'p str,
        year: Option<i32>,
        is_compilation: bool,
        created_at: String,
    ),
    r#"
    INSERT INTO albums (id, title, year, is_compilation, created_at)
    VALUES (:id, :title, :year, :is_compilation, :created_at)
    ON CONFLICT (id) DO UPDATE SET
        title = excluded.title,
        year = excluded.year,
        is_compilation = excluded.is_compilation
"#
);

/// Write `album` whether or not its row is here: a group's album (whose id is
/// the group's) can be here already, written by another import of the group.
#[cfg(desktop)]
pub fn upsert_album_row(conn: &WriteContext<'_>, album: &DbAlbum) -> Result<(), DbError> {
    conn.run(&UpsertAlbumParams {
        id: &album.id,
        title: &album.title,
        year: album.year,
        is_compilation: album.is_compilation,
        created_at: album.created_at.to_rfc3339(),
    })?;
    Ok(())
}

#[cfg(any(test, feature = "test-utils", all(debug_assertions, desktop)))]
pub(super) fn insert_album_artist_row(
    conn: &WriteContext<'_>,
    aa: &DbAlbumArtist,
) -> Result<(), DbError> {
    insert_album_credit(conn, aa, &aa.created_at.to_rfc3339())
}

coven::write!(
    InsertRelease(
        id: &'p str,
        album_id: String,
        release_name: Option<&'p str>,
        year: Option<i32>,
        country: Option<&'static str>,
        region: Option<&'static str>,
        status: Option<&'static str>,
        packaging: Option<&'static str>,
        source_folder_name: Option<&'p str>,
        album_loudness_lufs: Option<f64>,
        album_peak_linear: Option<f64>,
        created_at: String,
    ),
    r#"
    INSERT INTO releases (
        id, album_id, release_name, year,
        country, region, status, packaging,
        source_folder_name,
        album_loudness_lufs, album_peak_linear,
        created_at, album_order
    ) VALUES (
        :id, :album_id, :release_name, :year,
        :country, :region, :status, :packaging,
        :source_folder_name,
        :album_loudness_lufs, :album_peak_linear,
        :created_at, @crate::records::write::NextAlbumOrder(album: :album_id)
    )
"#
);

/// The release is ordered after its album's others, so an album's first
/// release stays its primary one. Its labels name the library labels
/// `labels` decides.
pub fn insert_release_row(
    conn: &WriteContext<'_>,
    release: &DbRelease,
    pressing: &super::PressingWrite<'_>,
) -> Result<(), IdentityWriteError> {
    let facts = super::pressing_columns::FactColumns::of(&release.pressing.facts);
    conn.run(&InsertReleaseParams {
        id: &release.id,
        album_id: release.album_id.clone(),
        release_name: release.release_name.as_deref(),
        year: release.pressing.typed.year,
        country: facts.country,
        region: facts.region,
        status: facts.status,
        packaging: facts.packaging,
        source_folder_name: release.source_folder_name.as_deref(),
        album_loudness_lufs: release.album_loudness_lufs,
        album_peak_linear: release.album_peak_linear,
        created_at: release.created_at.to_rfc3339(),
    })?;
    super::label_resolution::write_release_labels_on(
        conn,
        &release.id,
        &release.pressing.typed.labels,
        pressing,
    )?;
    replace_release_barcodes(
        conn,
        &release.id,
        &release.pressing.typed.barcodes,
        pressing.barcode_ids,
    )?;
    replace_release_media(conn, &release.id, &release.pressing.facts.media, pressing.media_ids)?;
    replace_release_discogs_details(
        conn,
        &release.id,
        &release.pressing.facts.discogs_details,
        pressing.discogs_detail_ids,
    )?;
    Ok(())
}

coven::write!(
    ClearReleaseLabels(release_id: &'p str),
    "DELETE FROM release_labels WHERE release_id = :release_id"
);

coven::write!(
    InsertReleaseLabel(
        id: &'p str,
        release_id: &'p str,
        position: i64,
        label_id: Option<&'p str>,
        catalog_number: Option<&'p str>,
    ),
    r#"
    INSERT INTO release_labels (id, release_id, position, label_id, catalog_number)
    VALUES (:id, :release_id, :position, :label_id, :catalog_number)
"#
);

/// Make the release `release_id`'s label rows exactly `rows`, each the id of
/// the library label it names and the catalog number it gives, in order, each
/// row under its id of `row_ids`.
pub(super) fn replace_release_label_rows(
    conn: &WriteContext<'_>,
    release_id: &str,
    rows: &[(Option<String>, Option<&str>)],
    row_ids: &[String],
) -> Result<(), DbError> {
    replace_release_list(
        conn,
        &ClearReleaseLabelsParams { release_id },
        rows,
        row_ids,
        |(label_id, catalog_number), id, position| {
            conn.run(&InsertReleaseLabelParams {
                id,
                release_id,
                position,
                label_id: label_id.as_deref(),
                catalog_number: *catalog_number,
            })
        },
    )
}

coven::write!(
    ClearReleaseBarcodes(release_id: &'p str),
    "DELETE FROM release_barcodes WHERE release_id = :release_id"
);

coven::write!(
    InsertReleaseBarcode(id: &'p str, release_id: &'p str, position: i64, barcode: &'p str),
    r#"
    INSERT INTO release_barcodes (id, release_id, position, barcode)
    VALUES (:id, :release_id, :position, :barcode)
"#
);

/// Make the release `release_id`'s barcodes exactly `barcodes`, in order,
/// each row under its id of `row_ids`.
pub(super) fn replace_release_barcodes(
    conn: &WriteContext<'_>,
    release_id: &str,
    barcodes: &[String],
    row_ids: &[String],
) -> Result<(), DbError> {
    replace_release_list(
        conn,
        &ClearReleaseBarcodesParams { release_id },
        barcodes,
        row_ids,
        |barcode, id, position| {
            conn.run(&InsertReleaseBarcodeParams { id, release_id, position, barcode })
        },
    )
}

coven::write!(
    ClearReleaseMedia(release_id: &'p str),
    "DELETE FROM release_media WHERE release_id = :release_id"
);

coven::write!(
    InsertReleaseMediaCount(
        id: &'p str,
        release_id: &'p str,
        position: i64,
        medium: &'static str,
        count: i64,
    ),
    r#"
    INSERT INTO release_media (id, release_id, position, medium, count)
    VALUES (:id, :release_id, :position, :medium, :count)
"#
);

/// Make the release `release_id`'s media exactly `media`, in order, each row
/// under its id of `row_ids`.
pub(super) fn replace_release_media(
    conn: &WriteContext<'_>,
    release_id: &str,
    media: &[bae_types::pressing::MediaCount],
    row_ids: &[String],
) -> Result<(), DbError> {
    replace_release_list(
        conn,
        &ClearReleaseMediaParams { release_id },
        media,
        row_ids,
        |counted, id, position| {
            conn.run(&InsertReleaseMediaCountParams {
                id,
                release_id,
                position,
                medium: counted.medium.key(),
                count: i64::from(counted.count),
            })
        },
    )
}

coven::write!(
    ClearReleaseDiscogsDetails(release_id: &'p str),
    "DELETE FROM release_discogs_details WHERE release_id = :release_id"
);

coven::write!(
    InsertReleaseDiscogsDetail(
        id: &'p str,
        release_id: &'p str,
        position: i64,
        detail: &'static str,
    ),
    r#"
    INSERT INTO release_discogs_details (id, release_id, position, detail)
    VALUES (:id, :release_id, :position, :detail)
"#
);

/// Make the release `release_id`'s Discogs details exactly `details`, in
/// order, each row under its id of `row_ids`.
pub(super) fn replace_release_discogs_details(
    conn: &WriteContext<'_>,
    release_id: &str,
    details: &[bae_types::pressing::DiscogsDetail],
    row_ids: &[String],
) -> Result<(), DbError> {
    replace_release_list(
        conn,
        &ClearReleaseDiscogsDetailsParams { release_id },
        details,
        row_ids,
        |detail, id, position| {
            conn.run(&InsertReleaseDiscogsDetailParams {
                id,
                release_id,
                position,
                detail: detail.key(),
            })
        },
    )
}

/// Replace one of the release's lists: run `clear`, which deletes the rows
/// the release has in it, then `insert` once for each of `rows`, in order,
/// with its id of `row_ids` and its position, numbered from zero.
fn replace_release_list<R>(
    conn: &WriteContext<'_>,
    clear: &impl coven::Write,
    rows: &[R],
    row_ids: &[String],
    insert: impl Fn(&R, &str, i64) -> rusqlite::Result<usize>,
) -> Result<(), DbError> {
    assert_eq!(rows.len(), row_ids.len(), "a write holds an id for each row of a release's list");
    conn.run(clear)?;
    for (position, (row, id)) in rows.iter().zip(row_ids).enumerate() {
        insert(row, id, position as i64)?;
    }
    Ok(())
}

coven::write!(
    InsertReleaseMedium(
        id: &'p str,
        release_id: &'p str,
        position: std::num::NonZeroU16,
        medium: Option<&'static str>,
        side_count: std::num::NonZeroU8,
        created_at: String,
    ),
    r#"
    INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at)
    VALUES (:id, :release_id, :position, :medium, :side_count, :created_at)
"#
);

pub fn insert_release_medium_row(
    conn: &WriteContext<'_>,
    medium: &DbReleaseMedium,
) -> Result<(), DbError> {
    conn.run(&InsertReleaseMediumParams {
        id: &medium.id,
        release_id: &medium.release_id,
        position: medium.position,
        medium: medium.medium.carrier.map(bae_types::pressing::Medium::key),
        side_count: medium.medium.sides,
        created_at: medium.created_at.to_rfc3339(),
    })?;
    Ok(())
}

// The position and id of each of the release `release`'s mediums, in order.
#[cfg(desktop)]
coven::read!(
    writer MediumsHeldBy(release: String) { position: u16 },
    r#"
    SELECT medium.position, medium.id
    FROM release_mediums AS medium
    WHERE medium.release_id = :release
    ORDER BY medium.position
"#
);

/// The position and id of each of the release `release_id`'s mediums, in
/// order.
#[cfg(desktop)]
pub(super) fn mediums_held_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Vec<(u16, String)>, DbError> {
    let statement = MediumsHeldByParams { release: release_id.to_owned() };
    Ok(bae_store::statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| (row.position, row.id))
        .collect())
}

#[cfg(desktop)]
coven::write!(
    SetReleaseMedium(id: &'p str, medium: Option<&'static str>, side_count: std::num::NonZeroU8),
    "UPDATE release_mediums SET medium = :medium, side_count = :side_count WHERE id = :id"
);

/// Write what `medium` is over the medium row `id`.
#[cfg(desktop)]
pub(super) fn update_release_medium_row(
    conn: &WriteContext<'_>,
    id: &str,
    medium: &bae_types::mediums::ReleaseMedium,
) -> Result<(), DbError> {
    conn.run(&SetReleaseMediumParams {
        id,
        medium: medium.carrier.map(bae_types::pressing::Medium::key),
        side_count: medium.sides,
    })?;
    Ok(())
}

#[cfg(desktop)]
coven::write!(DeleteReleaseMedium(id: &'p str), "DELETE FROM release_mediums WHERE id = :id");

#[cfg(desktop)]
pub(super) fn delete_release_medium_row(conn: &WriteContext<'_>, id: &str) -> Result<(), DbError> {
    conn.run(&DeleteReleaseMediumParams { id })?;
    Ok(())
}

// The track goes after the release's others.
coven::write!(
    InsertTrack(
        id: &'p str,
        release_id: &'p str,
        title: &'p str,
        medium_id: &'p str,
        side: std::num::NonZeroU8,
        track_number: std::num::NonZeroU32,
        duration_ms: u64,
        stated_length_ms: Option<u64>,
        created_at: String,
    ),
    r#"
    INSERT INTO tracks (
        id, release_id, title, medium_id, side, track_number, duration_ms,
        stated_length_ms, created_at, position
    ) VALUES (
        :id, :release_id, :title, :medium_id, :side, :track_number, :duration_ms,
        :stated_length_ms, :created_at,
        (SELECT COALESCE(MAX(position) + 1, 0) FROM tracks WHERE release_id = :release_id)
    )
"#
);

pub fn insert_track_row(conn: &WriteContext<'_>, track: &DbTrack) -> Result<(), DbError> {
    conn.run(&InsertTrackParams {
        id: &track.id,
        release_id: &track.release_id,
        title: &track.title,
        medium_id: &track.medium_id,
        side: track.side,
        track_number: track.track_number,
        duration_ms: track.duration_ms,
        stated_length_ms: track.stated_length_ms,
        created_at: track.created_at.to_rfc3339(),
    })?;
    Ok(())
}

pub fn insert_track_artist_row(conn: &WriteContext<'_>, ta: &DbTrackArtist) -> Result<(), DbError> {
    insert_track_credit(conn, ta, &ta.created_at.to_rfc3339())
}

#[cfg(any(test, feature = "test-utils", desktop))]
coven::write!(
    ClearReleaseRecords(release_id: &'p str),
    "DELETE FROM release_records WHERE release_id = :release_id"
);

#[cfg(any(test, feature = "test-utils", desktop))]
coven::write!(
    InsertReleaseRecord(
        id: String,
        release_id: &'p str,
        catalog: &'static str,
        kind: &'static str,
        key: &'p str,
        album_key: Option<&'p str>,
    ),
    r#"
    INSERT INTO release_records (id, release_id, catalog, kind, key, album_key)
    VALUES (:id, :release_id, :catalog, :kind, :key, :album_key)
"#
);

/// Write `records` as `release_id`'s records, replacing any it had, and set the
/// catalog its draft was read from. Shared by an import's commit, a records
/// change (`set_records_atomic`) and the test seam `insert_release_records`. Each row's id is its
/// `(release, catalog)`: one catalog's record of a release is one row.
#[cfg(any(test, feature = "test-utils", desktop))]
pub fn write_release_records(
    conn: &WriteContext<'_>,
    release_id: &str,
    records: &[bae_catalogs::records::ReleaseRecord],
) -> Result<(), DbError> {
    conn.run(&ClearReleaseRecordsParams { release_id })?;
    for record in records {
        let (kind, album_key) = match record {
            bae_catalogs::records::ReleaseRecord::Pressing { album_key, .. } => {
                ("pressing", album_key.as_deref())
            }
            bae_catalogs::records::ReleaseRecord::Album { .. } => ("album", None),
        };
        conn.run(&InsertReleaseRecordParams {
            id: crate::identity::release_record_id(release_id, record.catalog()),
            release_id,
            catalog: record.catalog().as_str(),
            kind,
            key: record.key(),
            album_key,
        })?;
    }
    Ok(())
}

coven::write!(
    ClearAlbumArtists(album_id: &'p str),
    "DELETE FROM album_artists WHERE album_id = :album_id"
);

/// Make `album_id`'s credit list exactly `artists`. An artist of another
/// album is a bug of the caller's, which panics in the store's work.
pub fn replace_album_artists(
    conn: &WriteContext<'_>,
    album_id: &str,
    artists: &[DbAlbumArtist],
    now: &str,
) -> Result<(), DbError> {
    if let Some(stray) = artists.iter().find(|aa| aa.album_id != album_id) {
        panic!(
            "an album artist names album {}, whose credits this write does not replace",
            stray.album_id
        );
    }
    conn.run(&ClearAlbumArtistsParams { album_id })?;
    for aa in artists {
        insert_album_credit(conn, aa, now)?;
    }
    Ok(())
}

#[cfg(desktop)]
coven::write!(
    ClearTrackArtists(track_id: &'p str),
    "DELETE FROM track_artists WHERE track_id = :track_id"
);

#[cfg(desktop)]
/// Make each track of `track_ids` credit exactly its `artists`. Callers pass
/// the affected track ids explicitly because `artists` may not cover every
/// track (a track legitimately has no per-track artists when it inherits from
/// the album). An artist of a track outside `track_ids` is a bug of the
/// caller's, which panics in the store's work.
pub(super) fn replace_track_artists(
    conn: &WriteContext<'_>,
    track_ids: &[&str],
    artists: &[DbTrackArtist],
    now: &str,
) -> Result<(), DbError> {
    if let Some(stray) = artists.iter().find(|ta| !track_ids.contains(&ta.track_id.as_str())) {
        panic!(
            "a track artist names track {}, whose credits this write does not replace",
            stray.track_id
        );
    }
    for track_id in track_ids {
        conn.run(&ClearTrackArtistsParams { track_id })?;
        for ta in artists.iter().filter(|ta| ta.track_id == *track_id) {
            insert_track_credit(conn, ta, now)?;
        }
    }
    Ok(())
}

coven::write!(
    InsertAlbumArtist(
        id: &'p str,
        album_id: &'p str,
        artist_id: &'p str,
        position: u32,
        credit_key: Option<String>,
        join_phrase: Option<&'p str>,
        created_at: &'p str,
    ),
    r#"
    INSERT INTO album_artists
        (id, album_id, artist_id, position, credit_key, join_phrase, created_at)
    VALUES (:id, :album_id, :artist_id, :position, :credit_key, :join_phrase, :created_at)
"#
);

/// Insert `credit` as a place of its album's credit list, made at `created_at`.
fn insert_album_credit(
    conn: &WriteContext<'_>,
    credit: &DbAlbumArtist,
    created_at: &str,
) -> Result<(), DbError> {
    let credit_key =
        settle_credit_name(conn, &credit.artist_id, credit.credit.as_ref(), created_at)?;
    conn.run(&InsertAlbumArtistParams {
        id: &credit.id,
        album_id: &credit.album_id,
        artist_id: &credit.artist_id,
        position: credit.position,
        credit_key,
        join_phrase: credit.join_phrase.as_deref(),
        created_at,
    })?;
    Ok(())
}

coven::write!(
    InsertTrackArtist(
        id: &'p str,
        track_id: &'p str,
        artist_id: &'p str,
        position: u32,
        credit_key: Option<String>,
        join_phrase: Option<&'p str>,
        created_at: &'p str,
    ),
    r#"
    INSERT INTO track_artists
        (id, track_id, artist_id, position, credit_key, join_phrase, created_at)
    VALUES (:id, :track_id, :artist_id, :position, :credit_key, :join_phrase, :created_at)
"#
);

/// Insert `credit` as a place of its track's credit list, made at `created_at`.
fn insert_track_credit(
    conn: &WriteContext<'_>,
    credit: &DbTrackArtist,
    created_at: &str,
) -> Result<(), DbError> {
    let credit_key =
        settle_credit_name(conn, &credit.artist_id, credit.credit.as_ref(), created_at)?;
    conn.run(&InsertTrackArtistParams {
        id: &credit.id,
        track_id: &credit.track_id,
        artist_id: &credit.artist_id,
        position: credit.position,
        credit_key,
        join_phrase: credit.join_phrase.as_deref(),
        created_at,
    })?;
    Ok(())
}
