use super::*;
#[cfg(any(test, feature = "test-utils", desktop))]
use rusqlite::Params;

/// The `album_order` that puts a release last among those of the album whose
/// id `album_id` names (a SQL expression; a bound parameter or a column).
pub(super) fn next_album_order_sql(album_id: &str) -> String {
    format!("(SELECT COALESCE(MAX(album_order) + 1, 0) FROM releases WHERE album_id = {album_id})")
}

pub(super) fn insert_artist_row(conn: &CovenWrite<'_>, artist: &DbArtist) -> Result<(), DbError> {
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
    conn: &CovenWrite<'_>,
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

// Callers: the test writes, and the debug build's seeded albums.
#[cfg(any(test, feature = "test-utils", all(debug_assertions, desktop)))]
pub(super) fn insert_album_row(conn: &CovenWrite<'_>, album: &DbAlbum) -> Result<(), DbError> {
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
/// the group's) can be here already, written by another import of the group.
#[cfg(desktop)]
pub fn upsert_album_row(conn: &CovenWrite<'_>, album: &DbAlbum) -> Result<(), DbError> {
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

#[cfg(any(test, feature = "test-utils", all(debug_assertions, desktop)))]
pub(super) fn insert_album_artist_row(
    conn: &CovenWrite<'_>,
    aa: &DbAlbumArtist,
) -> Result<(), DbError> {
    let created_at = aa.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &aa.artist_id, aa.credit.as_ref(), &created_at)?;
    let credit = Credit {
        id: aa.id(),
        artist_id: &aa.artist_id,
        position: aa.position,
        credit_key,
        join_phrase: aa.join_phrase.as_deref(),
    };
    insert_credit_row(conn, "album_artists", "album_id", &aa.album_id, &credit, &created_at)
}

/// `source_folder_name` names the folder an export writes under the user's
/// target directory, so, like a file's name (see [`insert_release_file_row`]),
/// it must name one. The release is ordered after its album's others, so an
/// album's first release stays its primary one. Its labels name the library
/// labels `labels` decides.
pub fn insert_release_row(
    conn: &CovenWrite<'_>,
    release: &DbRelease,
    labels: &super::LabelWrite<'_>,
) -> Result<(), IdentityWriteError> {
    if let Some(folder) = &release.source_folder_name {
        crate::stored_name::folder_path(&release.id, folder)?;
    }
    let facts = super::pressing_columns::FactColumns::of(&release.pressing.facts);
    conn.execute(
        &format!(
            r#"
        INSERT INTO releases (
            id, album_id, release_name, year,
            country, region, status, packaging, discogs_details,
            source_folder_name,
            album_loudness_lufs, album_peak_linear,
            created_at, album_order
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, {next_album_order})
        "#,
            next_album_order = next_album_order_sql("?")
        ),
        params![
            release.id,
            release.album_id,
            release.release_name,
            release.pressing.year,
            facts.country,
            facts.region,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            release.source_folder_name,
            release.album_loudness_lufs,
            release.album_peak_linear,
            release.created_at.to_rfc3339(),
            release.album_id,
        ],
    )?;
    super::label_resolution::write_release_labels_on(
        conn,
        &release.id,
        &release.pressing.labels,
        labels,
    )?;
    replace_release_barcodes(conn, &release.id, &release.pressing.barcodes)?;
    replace_release_media(conn, &release.id, &release.pressing.facts.media)?;
    Ok(())
}

/// Make the release `release_id`'s barcodes exactly `barcodes`, in order,
/// touching only the rows that differ: a row is a place in the list, so
/// another device's edit of another place merges with this one.
pub(super) fn replace_release_barcodes(
    conn: &CovenWrite<'_>,
    release_id: &str,
    barcodes: &[String],
) -> Result<(), DbError> {
    let stored: HashMap<String, String> = conn
        .query(
            "SELECT id, barcode FROM release_barcodes WHERE release_id = ?",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .into_iter()
        .collect();
    let ids: Vec<String> = (0..barcodes.len())
        .map(|position| crate::identity::release_barcode_id(release_id, position))
        .collect();
    for id in stored.keys() {
        if !ids.contains(id) {
            conn.execute("DELETE FROM release_barcodes WHERE id = ?", params![id])?;
        }
    }
    for (position, (id, barcode)) in ids.iter().zip(barcodes).enumerate() {
        match stored.get(id) {
            Some(stored) if stored == barcode => {}
            Some(_) => {
                conn.execute(
                    "UPDATE release_barcodes SET barcode = ? WHERE id = ?",
                    params![barcode, id],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO release_barcodes (id, release_id, position, barcode) \
                     VALUES (?, ?, ?, ?)",
                    params![id, release_id, position as i64, barcode],
                )?;
            }
        }
    }
    Ok(())
}

/// Make the release `release_id`'s media exactly `media`, in order, touching
/// only the rows that differ: a row is a place in the list, so another
/// device's edit of another place merges with this one.
pub(super) fn replace_release_media(
    conn: &CovenWrite<'_>,
    release_id: &str,
    media: &[bae_types::pressing::MediaCount],
) -> Result<(), DbError> {
    let stored: HashMap<String, (String, i64)> = conn
        .query(
            "SELECT id, medium, count FROM release_media WHERE release_id = ?",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?))),
        )?
        .into_iter()
        .collect();
    let rows: Vec<(String, usize, &bae_types::pressing::MediaCount)> = media
        .iter()
        .enumerate()
        .map(|(position, count)| {
            (crate::identity::release_media_id(release_id, position), position, count)
        })
        .collect();
    for id in stored.keys() {
        if !rows.iter().any(|(row_id, _, _)| row_id == id) {
            conn.execute("DELETE FROM release_media WHERE id = ?", params![id])?;
        }
    }
    for (id, position, count) in &rows {
        let medium = count.medium.key();
        let quantity = i64::from(count.count);
        match stored.get(id) {
            Some((stored_medium, stored_count))
                if stored_medium == medium && *stored_count == quantity => {}
            Some(_) => {
                conn.execute(
                    "UPDATE release_media SET medium = ?, count = ? WHERE id = ?",
                    params![medium, quantity, id],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO release_media (id, release_id, position, medium, count) \
                     VALUES (?, ?, ?, ?, ?)",
                    params![id, release_id, *position as i64, medium, quantity],
                )?;
            }
        }
    }
    Ok(())
}

pub fn insert_release_medium_row(
    conn: &CovenWrite<'_>,
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

pub fn insert_track_row(conn: &CovenWrite<'_>, track: &DbTrack) -> Result<(), DbError> {
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

pub fn insert_track_artist_row(conn: &CovenWrite<'_>, ta: &DbTrackArtist) -> Result<(), DbError> {
    let created_at = ta.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &ta.artist_id, ta.credit.as_ref(), &created_at)?;
    let credit = Credit {
        id: ta.id(),
        artist_id: &ta.artist_id,
        position: ta.position,
        credit_key,
        join_phrase: ta.join_phrase.as_deref(),
    };
    insert_credit_row(conn, "track_artists", "track_id", &ta.track_id, &credit, &created_at)
}

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_work_row(conn: &CovenWrite<'_>, work: &DbWork) -> Result<(), DbError> {
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

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_work_artist_row(conn: &CovenWrite<'_>, link: &DbWorkArtist) -> Result<(), DbError> {
    let created_at = link.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &link.artist_id, link.credit.as_ref(), &created_at)?;
    let columns = link.role.columns();
    conn.execute(
        r#"
        INSERT OR IGNORE INTO work_artists (
            id, work_id, artist_id, position, source, role, role_words, qualifiers, credit_key,
            created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            link.id(),
            link.work_id,
            link.artist_id,
            link.position,
            link.source.as_str(),
            columns.role,
            columns.role_words,
            columns.qualifiers,
            credit_key,
            created_at
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_work_part_row(conn: &CovenWrite<'_>, part: &DbWorkPart) -> Result<(), DbError> {
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

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_track_work_row(conn: &CovenWrite<'_>, link: &DbTrackWork) -> Result<(), DbError> {
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

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_release_artist_role_row(
    conn: &CovenWrite<'_>,
    role: &DbReleaseArtistRole,
) -> Result<(), DbError> {
    let created_at = role.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &role.artist_id, role.credit.as_ref(), &created_at)?;
    let columns = role.role.columns();
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
            columns.role,
            columns.role_words,
            columns.qualifiers,
            credit_key,
            created_at
        ],
    )
}

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_track_artist_role_row(
    conn: &CovenWrite<'_>,
    role: &DbTrackArtistRole,
) -> Result<(), DbError> {
    let created_at = role.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &role.artist_id, role.credit.as_ref(), &created_at)?;
    let columns = role.role.columns();
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
            columns.role,
            columns.role_words,
            columns.qualifiers,
            credit_key,
            created_at
        ],
    )
}

#[cfg(any(test, feature = "test-utils", desktop))]
pub(super) fn insert_artist_role_row(
    conn: &CovenWrite<'_>,
    table: &'static str,
    target_column: &'static str,
    values: impl Params,
) -> Result<(), DbError> {
    let sql = format!(
        r#"
        INSERT INTO {table} (
            id, {target_column}, artist_id, position, source, role, role_words, qualifiers,
            credit_key, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#
    );
    conn.execute(&sql, values).map(|_| ()).map_err(DbError::from)
}

#[cfg(any(test, feature = "test-utils", desktop))]
const INSERT_RELEASE_FILE_SQL: &str = r#"
    INSERT INTO release_files (
        id, release_id, original_filename, content_type, file, role, document_kind,
        source_audio_layout, source_audio_content_type, source_audio_container,
        source_audio_duration_ms, source_audio_sample_rate_hz, source_audio_bits_per_sample,
        source_audio_bitrate_kbps, source_audio_channels,
        created_at
    ) VALUES (
        :id, :release_id, :original_filename, :content_type, :file, :role, :document_kind,
        :source_audio_layout, :source_audio_content_type, :source_audio_container,
        :source_audio_duration_ms, :source_audio_sample_rate_hz, :source_audio_bits_per_sample,
        :source_audio_bitrate_kbps, :source_audio_channels,
        :created_at
    )
"#;

/// Insert a release file row referencing `coven_file`, the coven file the
/// write staged for it, once its name is checked to name a file.
#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_release_file_row(
    conn: &CovenWrite<'_>,
    file: &NewReleaseFile,
    coven_file: coven::FileId,
) -> Result<(), IdentityWriteError> {
    crate::stored_name::file_path(&file.release_id, &file.id, &file.original_filename)?;
    let source_audio_layout =
        file.source_audio.as_ref().and_then(|audio| audio.layout).map(|layout| match layout {
            crate::source_audio::SourceAudioLayout::File => "file",
            crate::source_audio::SourceAudioLayout::Cue => "cue",
        });
    let source_audio_content_type =
        file.source_audio.as_ref().map(|audio| audio.content_type.as_str());
    let source_audio_container = file.source_audio.as_ref().map(|audio| audio.container.as_str());
    let source_audio_duration_ms = file.source_audio.as_ref().map(|audio| audio.duration_ms);
    let source_audio_sample_rate_hz =
        file.source_audio.as_ref().map(|audio| audio.format.sample_rate_hz);
    let source_audio_bits_per_sample =
        file.source_audio.as_ref().and_then(|audio| audio.format.bits_per_sample);
    let source_audio_bitrate_kbps =
        file.source_audio.as_ref().and_then(|audio| audio.format.bitrate_kbps);
    let source_audio_channels = file.source_audio.as_ref().map(|audio| audio.format.channels);
    let created_at = file.created_at.to_rfc3339();
    conn.execute(
        INSERT_RELEASE_FILE_SQL,
        rusqlite::named_params! {
            ":id": file.id,
            ":release_id": file.release_id,
            ":original_filename": file.original_filename,
            ":content_type": file.content_type.as_str(),
            ":file": coven_file,
            ":role": file.role.as_str(),
            ":document_kind": file.role.document_kind().map(crate::DocumentKind::as_str),
            ":source_audio_layout": source_audio_layout,
            ":source_audio_content_type": source_audio_content_type,
            ":source_audio_container": source_audio_container,
            ":source_audio_duration_ms": source_audio_duration_ms,
            ":source_audio_sample_rate_hz": source_audio_sample_rate_hz,
            ":source_audio_bits_per_sample": source_audio_bits_per_sample,
            ":source_audio_bitrate_kbps": source_audio_bitrate_kbps,
            ":source_audio_channels": source_audio_channels,
            ":created_at": created_at,
        },
    )?;
    Ok(())
}

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_audio_format_row(conn: &CovenWrite<'_>, af: &DbAudioFormat) -> Result<(), DbError> {
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

#[cfg(any(test, feature = "test-utils", desktop))]
pub fn insert_audio_segment_row(
    conn: &CovenWrite<'_>,
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
            to_i64(segment.start_sample, "start_sample")?,
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
pub fn insert_scrub_points_row(
    conn: &CovenWrite<'_>,
    points: &DbScrubPoints,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO audio_segment_scrub_points (audio_format_segment_id, points) VALUES (?, ?)",
        params![points.audio_segment_id, super::scrub_points_column::encode(&points.points),],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// [`to_i64`] of a value that may be absent.
#[cfg(any(test, feature = "test-utils", desktop))]
fn optional_sqlite_count(value: Option<u64>, what: &str) -> Result<Option<i64>, DbError> {
    value.map(|value| to_i64(value, what)).transpose()
}

/// Write an image's row, pointing it at `file`, the coven file its write
/// staged.
pub fn upsert_library_image_row(
    conn: &CovenWrite<'_>,
    image: &DbLibraryImage,
    file: &coven::FileId,
) -> Result<(), DbError> {
    let table = image_table(&image.image_type);
    conn.execute(
        &format!(
            "INSERT INTO {table} (id, file, content_type) VALUES (?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
                 file = excluded.file, \
                 content_type = excluded.content_type"
        ),
        params![image.id, file, image.content_type.as_str()],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Write `records` as `release_id`'s records, replacing any it had, and set the
/// catalog its draft was read from. Shared by an import's commit, a records
/// change (`set_records_atomic`) and the test seam `insert_release_records`. Each row's id is its `(release, catalog)`, so a
/// device that writes the same record set another device wrote writes the same
/// rows.
#[cfg(any(test, feature = "test-utils", desktop))]
pub fn write_release_records(
    conn: &CovenWrite<'_>,
    release_id: &str,
    records: &[bae_catalogs::records::ReleaseRecord],
) -> Result<(), DbError> {
    conn.execute("DELETE FROM release_records WHERE release_id = ?", params![release_id])?;
    for record in records {
        let (kind, album_key) = match record {
            bae_catalogs::records::ReleaseRecord::Pressing { album_key, .. } => {
                ("pressing", album_key.as_deref())
            }
            bae_catalogs::records::ReleaseRecord::Album { .. } => ("album", None),
        };
        conn.execute(
            r#"
            INSERT INTO release_records (
                id, release_id, catalog, kind, key, album_key
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            params![
                crate::identity::release_record_id(release_id, record.catalog()),
                release_id,
                record.catalog().as_str(),
                kind,
                record.key(),
                album_key,
            ],
        )?;
    }
    Ok(())
}

/// Make `album_id`'s credit list exactly `artists`, touching only the rows
/// that differ (see `write_credits`). An artist of another album is a bug
/// of the caller's, which panics in the store's work.
pub fn replace_album_artists(
    conn: &CovenWrite<'_>,
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
/// the rows that differ (see `write_credits`). Callers pass the affected
/// track ids explicitly because `artists` may not cover every track (a track
/// legitimately has no per-track artists when it inherits from the album).
/// An artist of a track outside `track_ids` is a bug of the caller's, which
/// panics in the store's work.
pub(super) fn replace_track_artists(
    conn: &CovenWrite<'_>,
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

/// Insert `credit` as a place of `owner_id`'s list in `table`.
fn insert_credit_row(
    conn: &CovenWrite<'_>,
    table: &'static str,
    owner_column: &'static str,
    owner_id: &str,
    credit: &Credit<'_>,
    created_at: &str,
) -> Result<(), DbError> {
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
            created_at
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// What one stored place says: its artist, credit name and join.
type StoredCredit = (String, Option<String>, Option<String>);

/// Make `owner_id`'s rows of `table` exactly `credits`. A place no longer
/// given is deleted, a new one inserted, and one given again saying anything
/// else (another artist, credit name or join) rewritten; one given again as
/// it stands is not touched. So a write that keeps a credit leaves its row,
/// and its `created_at`, as they were, and syncs nothing for it.
fn write_credits(
    conn: &CovenWrite<'_>,
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
            None => insert_credit_row(conn, table, owner_column, owner_id, credit, now)?,
        }
    }
    Ok(())
}
