use super::*;

pub(super) fn insert_artist_row(
    conn: &SqlContext<'_, '_>,
    artist: &DbArtist,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO artists (
            id, name, name_key, sort_name, discogs_artist_id,
            musicbrainz_artist_id, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            artist.id,
            artist.name,
            bae_types::text_match::artist_name_key(&artist.name),
            artist.sort_name,
            artist.discogs_artist_id,
            artist.musicbrainz_artist_id,
            artist.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn update_artist_external_ids_row(
    conn: &SqlContext<'_, '_>,
    id: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
    sort_name: Option<&str>,
) -> Result<usize, DbError> {
    conn.execute(
        r#"
        UPDATE artists SET
            discogs_artist_id = COALESCE(discogs_artist_id, ?),
            musicbrainz_artist_id = COALESCE(musicbrainz_artist_id, ?),
            sort_name = COALESCE(sort_name, ?)
        WHERE id = ?
        "#,
        params![discogs_artist_id, musicbrainz_artist_id, sort_name, id,],
    )
    .map_err(DbError::from)
}

pub(super) fn insert_album_row(conn: &SqlContext<'_, '_>, album: &DbAlbum) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO albums (
            id, title, year, is_compilation, created_at
        ) VALUES (?, ?, ?, ?, ?)
        "#,
        params![
            album.id,
            album.title,
            album.year,
            album.is_compilation,
            album.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Write `album` whether or not its row is here: a group's album (whose id is
/// the group's) can be here already, emptied, when a release is filed under it
/// again.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn upsert_album_row(conn: &SqlContext<'_, '_>, album: &DbAlbum) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO albums (
            id, title, year, is_compilation, created_at
        ) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT (id) DO UPDATE SET
            title = excluded.title,
            year = excluded.year,
            is_compilation = excluded.is_compilation
        "#,
        params![
            album.id,
            album.title,
            album.year,
            album.is_compilation,
            album.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn insert_album_artist_row(
    conn: &SqlContext<'_, '_>,
    aa: &DbAlbumArtist,
) -> Result<(), DbError> {
    let created_at = aa.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &aa.artist_id, aa.credit.as_ref(), &created_at)?;
    conn.execute(
        r#"
        INSERT INTO album_artists
            (id, album_id, artist_id, position, credit_key, join_phrase, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            aa.id(),
            aa.album_id,
            aa.artist_id,
            aa.position,
            credit_key,
            aa.join_phrase,
            created_at
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// `source_folder_name` is the folder an export reconstructs under the user's
/// target directory, so it is held to the same fragment policy as a file's name
/// (see [`with_file_row_params`]). The release is ordered after its album's
/// others, so an album's first release stays its primary one.
pub(super) fn insert_release_row(
    conn: &SqlContext<'_, '_>,
    release: &DbRelease,
) -> Result<(), DbError> {
    if let Some(folder) = &release.source_folder_name {
        crate::storage::path_fragment::validate_path_fragment(
            &release.id,
            "source_folder_name",
            folder,
        )
        .map_err(|e| DbError::Message(e.to_string()))?;
    }
    let facts = super::pressing_columns::FactColumns::of(&release.pressing.facts);
    conn.execute(
        r#"
        INSERT INTO releases (
            id, album_id, release_name, year,
            labels, barcodes,
            country, region, media, status, packaging, discogs_details,
            source_folder_name,
            album_loudness_lufs, album_peak_linear,
            created_at, album_order
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
            (SELECT COALESCE(MAX(album_order) + 1, 0) FROM releases WHERE album_id = ?))
        "#,
        params![
            release.id,
            release.album_id,
            release.release_name,
            release.pressing.year,
            super::pressing_columns::labels_column(&release.pressing.labels),
            super::pressing_columns::barcodes_column(&release.pressing.barcodes),
            facts.country,
            facts.region,
            facts.media,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            release.source_folder_name,
            release.album_loudness_lufs,
            release.album_peak_linear,
            release.created_at.to_rfc3339(),
            release.album_id,
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn insert_release_medium_row(
    conn: &SqlContext<'_, '_>,
    medium: &DbReleaseMedium,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
        params![
            medium.id,
            medium.release_id,
            medium.position,
            medium.medium.carrier.map(bae_types::pressing::Medium::key),
            medium.medium.sides,
            medium.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn insert_track_row(conn: &SqlContext<'_, '_>, track: &DbTrack) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO tracks (
            id, release_id, title, medium_id, side, track_number, duration_ms,
            created_at, position
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?,
            (SELECT COALESCE(MAX(position) + 1, 0) FROM tracks WHERE release_id = ?))
        "#,
        params![
            track.id,
            track.release_id,
            track.title,
            track.medium_id,
            track.side,
            track.track_number,
            track.duration_ms,
            track.created_at.to_rfc3339(),
            track.release_id,
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn insert_track_artist_row(
    conn: &SqlContext<'_, '_>,
    ta: &DbTrackArtist,
) -> Result<(), DbError> {
    let created_at = ta.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &ta.artist_id, ta.credit.as_ref(), &created_at)?;
    conn.execute(
        r#"
        INSERT INTO track_artists
            (id, track_id, artist_id, position, credit_key, join_phrase, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            ta.id(),
            ta.track_id,
            ta.artist_id,
            ta.position,
            credit_key,
            ta.join_phrase,
            created_at
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
pub(super) fn insert_work_row(conn: &SqlContext<'_, '_>, work: &DbWork) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO works (
            id, title, disambiguation, work_type, musicbrainz_work_id, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            work.id,
            work.title,
            work.disambiguation,
            work.work_type,
            work.musicbrainz_work_id,
            work.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_work_artist_row(
    conn: &SqlContext<'_, '_>,
    link: &DbWorkArtist,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO work_artists (id, work_id, artist_id, position, source, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            link.id(),
            link.work_id,
            link.artist_id,
            link.position,
            link.source.as_str(),
            link.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_work_part_row(
    conn: &SqlContext<'_, '_>,
    part: &DbWorkPart,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO work_parts (
            id, parent_work_id, child_work_id, position, source, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            part.id(),
            part.parent_work_id,
            part.child_work_id,
            part.position,
            part.source.as_str(),
            part.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
pub(super) fn insert_track_work_row(
    conn: &SqlContext<'_, '_>,
    link: &DbTrackWork,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO track_works (id, track_id, work_id, position, source, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            link.id,
            link.track_id,
            link.work_id,
            link.position,
            link.source.as_str(),
            link.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_release_artist_role_row(
    conn: &SqlContext<'_, '_>,
    role: &DbReleaseArtistRole,
) -> Result<(), DbError> {
    insert_artist_role_row(
        conn,
        "release_artist_roles",
        "release_id",
        params![
            role.id,
            role.release_id,
            role.artist_id,
            role.position,
            role.source.as_str(),
            role.source_credit,
            role.created_at.to_rfc3339()
        ],
    )
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_track_artist_role_row(
    conn: &SqlContext<'_, '_>,
    role: &DbTrackArtistRole,
) -> Result<(), DbError> {
    insert_artist_role_row(
        conn,
        "track_artist_roles",
        "track_id",
        params![
            role.id,
            role.track_id,
            role.artist_id,
            role.position,
            role.source.as_str(),
            role.source_credit,
            role.created_at.to_rfc3339()
        ],
    )
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_artist_role_row(
    conn: &SqlContext<'_, '_>,
    table: &'static str,
    target_column: &'static str,
    values: impl Params,
) -> Result<(), DbError> {
    let sql = format!(
        r#"
        INSERT INTO {table} (
            id, {target_column}, artist_id, position, source, source_credit, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#
    );
    conn.execute(&sql, values)
        .map(|_| ())
        .map_err(DbError::from)
}

#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
const INSERT_RELEASE_FILE_SQL: &str = r#"
    INSERT INTO release_files (
        id, release_id, original_filename, file_size, content_type, file,
        source_audio_layout, source_audio_content_type, source_audio_duration_ms,
        source_audio_sample_rate_hz, source_audio_bits_per_sample,
        source_audio_bitrate_kbps, source_audio_channels,
        created_at
    ) VALUES (
        :id, :release_id, :original_filename, :file_size, :content_type, :file,
        :source_audio_layout, :source_audio_content_type, :source_audio_duration_ms,
        :source_audio_sample_rate_hz, :source_audio_bits_per_sample,
        :source_audio_bitrate_kbps, :source_audio_channels,
        :created_at
    )
"#;

/// Validate the file name and bind the row's columns, `file` the coven file
/// it references, or `None` for a file this device keeps local.
#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
fn with_file_row_params<T>(
    file: &DbFile,
    coven_file: Option<coven::FileId>,
    write: impl FnOnce(&[(&str, &dyn rusqlite::ToSql)]) -> Result<T, DbError>,
) -> Result<T, DbError> {
    crate::storage::path_fragment::validate_path_fragment(
        &file.release_id,
        &format!("original_filename for file {}", file.id),
        &file.original_filename,
    )
    .map_err(|e| DbError::Message(e.to_string()))?;
    let source_audio_layout = file
        .source_audio
        .as_ref()
        .and_then(|audio| audio.layout)
        .map(|layout| match layout {
            crate::album_detail::SourceAudioLayout::File => "file",
            crate::album_detail::SourceAudioLayout::Cue => "cue",
        });
    let source_audio_content_type = file
        .source_audio
        .as_ref()
        .map(|audio| audio.content_type.as_str());
    let source_audio_duration_ms = file.source_audio.as_ref().map(|audio| audio.duration_ms);
    let source_audio_sample_rate_hz = file
        .source_audio
        .as_ref()
        .map(|audio| audio.format.sample_rate_hz);
    let source_audio_bits_per_sample = file
        .source_audio
        .as_ref()
        .and_then(|audio| audio.format.bits_per_sample);
    let source_audio_bitrate_kbps = file
        .source_audio
        .as_ref()
        .and_then(|audio| audio.format.bitrate_kbps);
    let source_audio_channels = file
        .source_audio
        .as_ref()
        .map(|audio| audio.format.channels);
    let created_at = file.created_at.to_rfc3339();
    write(rusqlite::named_params! {
        ":id": file.id,
        ":release_id": file.release_id,
        ":original_filename": file.original_filename,
        ":file_size": file.file_size,
        ":content_type": file.content_type.as_str(),
        ":file": coven_file,
        ":source_audio_layout": source_audio_layout,
        ":source_audio_content_type": source_audio_content_type,
        ":source_audio_duration_ms": source_audio_duration_ms,
        ":source_audio_sample_rate_hz": source_audio_sample_rate_hz,
        ":source_audio_bits_per_sample": source_audio_bits_per_sample,
        ":source_audio_bitrate_kbps": source_audio_bitrate_kbps,
        ":source_audio_channels": source_audio_channels,
        ":created_at": created_at,
    })
}

/// Insert a release file row referencing `coven_file`, the coven file the
/// write staged from the person's original for the cloud, and record that
/// original on this device.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_external_file_row(
    conn: &SqlContext<'_, '_>,
    file: &DbFile,
    original: &coven::UserFile,
    coven_file: coven::FileId,
) -> Result<(), DbError> {
    with_file_row_params(file, Some(coven_file), |params| {
        conn.execute(INSERT_RELEASE_FILE_SQL, params)
            .map_err(DbError::from)
    })?;
    record_original_on(conn, file, original)
}

/// Insert a release file row whose original stays on this device, with no
/// coven file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_local_file_row(
    conn: &SqlContext<'_, '_>,
    file: &DbFile,
    original: &coven::UserFile,
) -> Result<(), DbError> {
    with_file_row_params(file, None, |params| {
        conn.execute(INSERT_RELEASE_FILE_SQL, params)
            .map_err(DbError::from)
    })?;
    record_original_on(conn, file, original)
}

/// Record on this device the original a release file was imported from.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn record_original_on(
    conn: &SqlContext<'_, '_>,
    file: &DbFile,
    original: &coven::UserFile,
) -> Result<(), DbError> {
    if original.size != file.file_size as u64 {
        return Err(DbError::Message(format!(
            "original size changed for {}",
            file.id
        )));
    }
    conn.execute(
        "INSERT INTO local_release_files (file_id, path, modified_at) VALUES (?, ?, ?)",
        params![
            file.id,
            coven::path_bytes(&original.path),
            chrono::DateTime::<chrono::Utc>::from(original.modified_at).to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Insert a file row naming no coven file. Tests that exercise bytes stage
/// a real file through the production import or move path.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn insert_file_row_for_test(
    conn: &SqlContext<'_, '_>,
    file: &DbFile,
) -> Result<(), DbError> {
    with_file_row_params(file, None, |params| {
        conn.execute(INSERT_RELEASE_FILE_SQL, params)
            .map(|_| ())
            .map_err(DbError::from)
    })
}

#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
pub(super) fn insert_audio_format_row(
    conn: &SqlContext<'_, '_>,
    af: &DbAudioFormat,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO audio_formats (
            id, track_id, content_type, pregap_samples, generated_pregap_samples, sample_rate, bits_per_sample, channels, track_loudness_lufs, track_peak_linear, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            af.id,
            af.track_id,
            af.content_type.as_str(),
            optional_sqlite_count(af.pregap_samples, "pregap_samples")?,
            optional_sqlite_count(af.generated_pregap_samples, "generated_pregap_samples")?,
            af.sample_rate.get(),
            af.bits_per_sample.map(std::num::NonZeroU32::get),
            af.channels.get(),
            af.track_loudness_lufs,
            af.track_peak_linear,
            af.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
pub(super) fn insert_audio_segment_row(
    conn: &SqlContext<'_, '_>,
    segment: &DbAudioSegment,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO audio_format_segments (
            id, audio_format_id, segment_index, role, file_id, start_sample, end_sample, start_byte, end_byte, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            segment.id,
            segment.audio_format_id,
            segment.segment_index,
            segment.role.as_str(),
            segment.file_id,
            sqlite_count(segment.start_sample, "start_sample")?,
            optional_sqlite_count(segment.end_sample, "end_sample")?,
            optional_sqlite_count(segment.start_byte, "start_byte")?,
            optional_sqlite_count(segment.end_byte, "end_byte")?,
            segment.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(any(test, feature = "test-utils", desktop))]
pub(super) fn insert_scrub_points_row(
    conn: &SqlContext<'_, '_>,
    points: &DbScrubPoints,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO audio_segment_scrub_points (audio_format_segment_id, points) VALUES (?, ?)",
        params![
            points.audio_segment_id,
            super::scrub_points_column::encode(&points.points)?,
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// A count or a position as SQLite's signed integer. One past `i64::MAX`
/// cannot be stored and fails the write.
#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
fn sqlite_count(value: u64, what: &str) -> Result<i64, DbError> {
    i64::try_from(value).map_err(|error| {
        DbError::Message(format!(
            "{what} {value} exceeds SQLite's integer range: {error}"
        ))
    })
}

#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
fn optional_sqlite_count(value: Option<u64>, what: &str) -> Result<Option<i64>, DbError> {
    value.map(|value| sqlite_count(value, what)).transpose()
}

/// Write an image's row, pointing it at `file`, the coven file its write
/// staged.
pub(super) fn upsert_library_image_row(
    conn: &SqlContext<'_, '_>,
    image: &DbLibraryImage,
    file: &coven::FileId,
) -> Result<(), DbError> {
    let table = image_table(&image.image_type);
    conn.execute(
        &format!(
            "INSERT INTO {table} (id, file, content_type, width, height, source, source_url, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
                 file = excluded.file, \
                 content_type = excluded.content_type, \
                 width = excluded.width, \
                 height = excluded.height, \
                 source = excluded.source, \
                 source_url = excluded.source_url"
        ),
        params![
            image.id,
            file,
            image.content_type.as_str(),
            image.width,
            image.height,
            image.source,
            image.source_url,
            image.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Write `records` as `release_id`'s records, replacing any it had, and set the
/// catalog its draft was read from. Shared by the atomic import path
/// (`finalize_import_atomic` / `set_records_atomic`) and the test seam
/// `insert_release_records`. Each row's id is its `(release, catalog)`, so a
/// device that writes the same record set another device wrote writes the same
/// rows.
#[cfg(any(
    test,
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
pub(super) fn write_release_records(
    conn: &SqlContext<'_, '_>,
    release_id: &str,
    records: &[crate::import::ReleaseRecord],
    now: &str,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM release_records WHERE release_id = ?",
        params![release_id],
    )?;
    for record in records {
        let (kind, album_key) = match record {
            crate::import::ReleaseRecord::Pressing { album_key, .. } => {
                ("pressing", album_key.as_deref())
            }
            crate::import::ReleaseRecord::Album { .. } => ("album", None),
        };
        conn.execute(
            r#"
            INSERT INTO release_records (
                id, release_id, catalog, kind, key, album_key,
                url, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                crate::db::identity::release_record_id(release_id, record.catalog()),
                release_id,
                record.catalog().as_str(),
                kind,
                record.key(),
                album_key,
                record.url(),
                now,
            ],
        )?;
    }
    Ok(())
}

/// Make `album_id`'s credit list exactly `artists`, touching only the rows
/// that differ (see [`write_credits`]).
pub(super) fn replace_album_artists(
    conn: &SqlContext<'_, '_>,
    album_id: &str,
    artists: &[DbAlbumArtist],
    now: &str,
) -> Result<(), DbError> {
    if let Some(stray) = artists.iter().find(|aa| aa.album_id != album_id) {
        return Err(DbError::Message(format!(
            "an album artist names album {}, whose credits this write does not replace",
            stray.album_id
        )));
    }
    let credits: Vec<Credit<'_>> = artists
        .iter()
        .map(|aa| {
            Ok(Credit {
                id: aa.id(),
                artist_id: &aa.artist_id,
                position: aa.position,
                credit_key: settle_credit_name(conn, &aa.artist_id, aa.credit.as_ref(), now)?,
                join_phrase: aa.join_phrase.as_deref(),
            })
        })
        .collect::<Result<_, DbError>>()?;
    write_credits(conn, "album_artists", "album_id", album_id, &credits, now)
}

/// Make each track of `track_ids` credit exactly its `artists`, touching only
/// the rows that differ (see [`write_credits`]). Callers pass the affected
/// track ids explicitly because `artists` may not cover every track (a track
/// legitimately has no per-track artists when it inherits from the album).
pub(super) fn replace_track_artists(
    conn: &SqlContext<'_, '_>,
    track_ids: &[&str],
    artists: &[DbTrackArtist],
    now: &str,
) -> Result<(), DbError> {
    if let Some(stray) = artists
        .iter()
        .find(|ta| !track_ids.contains(&ta.track_id.as_str()))
    {
        return Err(DbError::Message(format!(
            "a track artist names track {}, whose credits this write does not replace",
            stray.track_id
        )));
    }
    for track_id in track_ids {
        let credits: Vec<Credit<'_>> = artists
            .iter()
            .filter(|ta| ta.track_id == *track_id)
            .map(|ta| {
                Ok(Credit {
                    id: ta.id(),
                    artist_id: &ta.artist_id,
                    position: ta.position,
                    credit_key: settle_credit_name(conn, &ta.artist_id, ta.credit.as_ref(), now)?,
                    join_phrase: ta.join_phrase.as_deref(),
                })
            })
            .collect::<Result<_, DbError>>()?;
        write_credits(conn, "track_artists", "track_id", track_id, &credits, now)?;
    }
    Ok(())
}

/// One place of an `album_artists` / `track_artists` list. Its id derives
/// from the owner and the position, so the same place written again names
/// the same row.
struct Credit<'a> {
    id: String,
    artist_id: &'a str,
    position: u32,
    credit_key: Option<String>,
    join_phrase: Option<&'a str>,
}

/// What one stored place says: its artist, credit name and join.
type StoredCredit = (String, Option<String>, Option<String>);

/// Make `owner_id`'s rows of `table` exactly `credits`. A place no longer
/// given is deleted, a new one inserted, and one given again saying anything
/// else (another artist, credit name or join) rewritten; one given again as
/// it stands is not touched. So a write that keeps a credit leaves its row,
/// and its `created_at`, as they were, and syncs nothing for it.
fn write_credits(
    conn: &SqlContext<'_, '_>,
    table: &'static str,
    owner_column: &'static str,
    owner_id: &str,
    credits: &[Credit<'_>],
    now: &str,
) -> Result<(), DbError> {
    let stored: HashMap<String, StoredCredit> = conn
        .query(
            &format!(
                "SELECT id, artist_id, credit_key, join_phrase FROM {table} WHERE {owner_column} = ?"
            ),
            params![owner_id],
            |row| Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?, row.get(3)?))),
        )?
        .into_iter()
        .collect();
    for id in stored.keys() {
        if !credits.iter().any(|credit| credit.id == *id) {
            conn.execute(&format!("DELETE FROM {table} WHERE id = ?"), params![id])?;
        }
    }
    for credit in credits {
        match stored.get(&credit.id) {
            Some((artist_id, credit_key, join_phrase))
                if artist_id == credit.artist_id
                    && *credit_key == credit.credit_key
                    && join_phrase.as_deref() == credit.join_phrase => {}
            Some(_) => {
                conn.execute(
                    &format!(
                        "UPDATE {table} SET artist_id = ?, credit_key = ?, join_phrase = ? WHERE id = ?"
                    ),
                    params![credit.artist_id, credit.credit_key, credit.join_phrase, credit.id],
                )?;
            }
            None => {
                conn.execute(
                    &format!(
                        "INSERT INTO {table} \
                             (id, {owner_column}, artist_id, position, credit_key, join_phrase, created_at) \
                         VALUES (?, ?, ?, ?, ?, ?, ?)"
                    ),
                    params![
                        credit.id,
                        owner_id,
                        credit.artist_id,
                        credit.position,
                        credit.credit_key,
                        credit.join_phrase,
                        now
                    ],
                )?;
            }
        }
    }
    Ok(())
}
