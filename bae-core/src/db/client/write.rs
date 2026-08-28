use super::*;

pub(super) fn insert_artist_row(
    conn: &SqlContext<'_, '_>,
    artist: &DbArtist,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO artists (
            id, name, sort_name, discogs_artist_id,
            musicbrainz_artist_id, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            artist.id,
            artist.name,
            artist.sort_name,
            artist.discogs_artist_id,
            artist.musicbrainz_artist_id,
            reg,
            artist.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn upsert_library_image_row_with_cloud_path(
    conn: &SqlContext<'_, '_>,
    image: &DbLibraryImage,
    cloud_path: Option<String>,
    reg: &str,
) -> Result<(), DbError> {
    let image = DbLibraryImage {
        cloud_path,
        ..image.clone()
    };
    upsert_library_image_row(conn, &image, reg)
}

pub(super) fn update_artist_external_ids_row(
    conn: &SqlContext<'_, '_>,
    id: &str,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
    sort_name: Option<&str>,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        UPDATE artists SET
            discogs_artist_id = COALESCE(discogs_artist_id, ?),
            musicbrainz_artist_id = COALESCE(musicbrainz_artist_id, ?),
            sort_name = COALESCE(sort_name, ?),
            _updated_at = ?
        WHERE id = ?
        "#,
        params![discogs_artist_id, musicbrainz_artist_id, sort_name, reg, id,],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn insert_album_row(
    conn: &SqlContext<'_, '_>,
    album: &DbAlbum,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO albums (
            id, title, artist_id, year, primary_release_id, is_compilation,
            _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            album.id,
            album.title,
            album.artist_id,
            album.year,
            album.primary_release_id,
            album.is_compilation,
            reg,
            album.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn insert_album_artist_row(
    conn: &SqlContext<'_, '_>,
    aa: &DbAlbumArtist,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            aa.id,
            aa.album_id,
            aa.artist_id,
            aa.position,
            reg,
            aa.created_at.to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// `source_folder_name` is the folder an export reconstructs under the user's
/// target directory, so it is held to the same fragment policy as a file's name
/// (see [`insert_file_row`]).
pub(super) fn insert_release_row(
    conn: &SqlContext<'_, '_>,
    release: &DbRelease,
    reg: &str,
) -> Result<(), DbError> {
    if let Some(folder) = &release.source_folder_name {
        crate::storage::path_fragment::validate_path_fragment(
            &release.id,
            "source_folder_name",
            folder,
        )
        .map_err(|e| DbError::Message(e.to_string()))?;
    }
    let (metadata_source, metadata_source_release_id) =
        metadata_provenance_columns(release.metadata_provenance.as_ref());
    conn.execute(
        r#"
        INSERT INTO releases (
            id, album_id, release_name, year,
            disc_id, metadata_source, metadata_source_release_id,
            format, label, catalog_number, country, barcode,
            remote,
            source_folder_name, content_hash,
            album_loudness_lufs, album_peak_linear,
            _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            release.id,
            release.album_id,
            release.release_name,
            release.pressing.year,
            release.disc_id,
            metadata_source,
            metadata_source_release_id,
            release.pressing.format,
            release.pressing.label,
            release.pressing.catalog_number,
            release.pressing.country,
            release.pressing.barcode,
            release.remote,
            release.source_folder_name,
            release.content_hash,
            release.album_loudness_lufs,
            release.album_peak_linear,
            reg,
            release.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn metadata_provenance_columns(
    provenance: Option<&crate::import::MetadataProvenance>,
) -> (&str, Option<&str>) {
    match provenance {
        Some(crate::import::MetadataProvenance::ExternalRelease { source, release_id }) => {
            (source.as_str(), Some(release_id.as_str()))
        }
        Some(crate::import::MetadataProvenance::FileTags) => ("file_tags", None),
        None => ("none", None),
    }
}

pub(super) fn insert_track_row(
    conn: &SqlContext<'_, '_>,
    track: &DbTrack,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO tracks (
            id, release_id, title, side, track_number, duration_ms,
            discogs_position, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            track.id,
            track.release_id,
            track.title,
            track.side,
            track.track_number,
            track.duration_ms,
            track.discogs_position,
            reg,
            track.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn insert_track_artist_row(
    conn: &SqlContext<'_, '_>,
    ta: &DbTrackArtist,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            ta.id,
            ta.track_id,
            ta.artist_id,
            ta.position,
            reg,
            ta.created_at.to_rfc3339()
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
pub(super) fn insert_work_row(
    conn: &SqlContext<'_, '_>,
    work: &DbWork,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO works (
            id, title, disambiguation, work_type, musicbrainz_work_id, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            work.id,
            work.title,
            work.disambiguation,
            work.work_type,
            work.musicbrainz_work_id,
            reg,
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
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            link.id,
            link.work_id,
            link.artist_id,
            link.position,
            link.source.as_str(),
            reg,
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
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO work_parts (
            id, parent_work_id, child_work_id, position, source, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            part.id,
            part.parent_work_id,
            part.child_work_id,
            part.position,
            part.source.as_str(),
            reg,
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
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO track_works (id, track_id, work_id, position, source, _updated_at, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            link.id,
            link.track_id,
            link.work_id,
            link.position,
            link.source.as_str(),
            reg,
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
    reg: &str,
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
            reg,
            role.created_at.to_rfc3339()
        ],
    )
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) fn insert_track_artist_role_row(
    conn: &SqlContext<'_, '_>,
    role: &DbTrackArtistRole,
    reg: &str,
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
            reg,
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
            id, {target_column}, artist_id, position, source, source_credit, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#
    );
    conn.execute(&sql, values)
        .map(|_| ())
        .map_err(DbError::from)
}

/// The one row-write for `release_files`, so the one place `original_filename`
/// enters durable state. Export and make-Local join that column onto a directory
/// the user chose, and it syncs to every other device, so a name that can't be
/// materialized there is refused here — inside the caller's transaction, which
/// rolls back whole rather than committing a release nobody can copy out.
pub(super) fn insert_file_row(
    conn: &SqlContext<'_, '_>,
    file: &DbFile,
    reg: &str,
) -> Result<(), DbError> {
    crate::storage::path_fragment::validate_path_fragment(
        &file.release_id,
        &format!("original_filename for file {}", file.id),
        &file.original_filename,
    )
    .map_err(|e| DbError::Message(e.to_string()))?;
    conn.execute(
        r#"
        INSERT INTO release_files (
            id, release_id, original_filename, file_size, content_type, cloud_path, hash, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            file.id,
            file.release_id,
            file.original_filename,
            file.file_size,
            file.content_type.as_str(),
            file.cloud_path,
            file.content_hash,
            reg,
            file.created_at.to_rfc3339(),
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
pub(super) fn insert_audio_format_row(
    conn: &SqlContext<'_, '_>,
    af: &DbAudioFormat,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO audio_formats (
            id, track_id, content_type, pregap_ms, generated_pregap_ms, pregap_samples, generated_pregap_samples, sample_rate, bits_per_sample, channels, track_loudness_lufs, track_peak_linear, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            af.id,
            af.track_id,
            af.content_type.as_str(),
            af.pregap_ms,
            af.generated_pregap_ms,
            af.pregap_samples,
            af.generated_pregap_samples,
            af.sample_rate,
            af.bits_per_sample,
            af.channels,
            af.track_loudness_lufs,
            af.track_peak_linear,
            reg,
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
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO audio_format_segments (
            id, audio_format_id, segment_index, role, file_id, start_sample, end_sample, start_byte, end_byte, _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            segment.id,
            segment.audio_format_id,
            segment.segment_index,
            segment.role.as_str(),
            segment.file_id,
            segment.start_sample,
            segment.end_sample,
            segment.start_byte,
            segment.end_byte,
            reg,
            segment.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

pub(super) fn upsert_library_image_row(
    conn: &SqlContext<'_, '_>,
    image: &DbLibraryImage,
    reg: &str,
) -> Result<(), DbError> {
    let table = image_table(&image.image_type);
    conn.execute(
        &format!(
            "INSERT INTO {table} (id, blob_id, content_type, file_size, width, height, source, source_url, cloud_path, hash, _updated_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
                 blob_id = excluded.blob_id, \
                 content_type = excluded.content_type, \
                 file_size = excluded.file_size, \
                 width = excluded.width, \
                 height = excluded.height, \
                 source = excluded.source, \
                 source_url = excluded.source_url, \
                 cloud_path = excluded.cloud_path, \
                 hash = excluded.hash, \
                 _updated_at = excluded._updated_at"
        ),
        params![
            image.id,
            image.blob_id,
            image.content_type.as_str(),
            image.file_size,
            image.width,
            image.height,
            image.source,
            image.source_url,
            image.cloud_path,
            image.content_hash,
            reg,
            image.created_at.to_rfc3339(),
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// A release's `(album_id, source_folder_name)` — the release-scoped context a
/// browsable key is built from. The release row always exists when one of its
/// blobs is being keyed (it was just inserted), so a missing row is a broken
/// invariant surfaced as an error, not masked. `source_folder_name` is `None`
/// for a non-folder import.
pub(super) fn release_path_context<C: QueryOne>(
    conn: &C,
    release_id: &str,
) -> Result<(String, Option<String>), DbError> {
    conn.query_row(
        "SELECT album_id, source_folder_name FROM releases WHERE id = ?",
        params![release_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    )
    .map_err(DbError::from)
}

/// Build a release-scoped browsable key: look up the release's `(album_id,
/// source_folder)` context once, then let `make_key` shape the final path. Shared
/// by the audio and cover resolvers; the artist-image resolver stands apart, keyed
/// off the artist id alone.
pub(super) fn resolve_release_path<C: QueryOne>(
    conn: &C,
    release_id: &str,
    make_key: impl FnOnce(&str, &str, Option<&str>) -> String,
) -> Result<String, DbError> {
    let (album_id, source_folder) = release_path_context(conn, release_id)?;
    Ok(make_key(&album_id, release_id, source_folder.as_deref()))
}

/// The `cloud_path` for a release file on a browsable home:
/// `{album_id}/{release_id}/{source_folder}/{filename}` (relative to the
/// `release_files` namespace coven prepends), mirroring the imported folder. Ids
/// are immutable and unique, so the key is stable and collision-free by
/// construction — no disambiguation.
#[cfg(any(test, not(any(target_os = "ios", target_os = "android"))))]
pub(super) fn resolve_audio_cloud_path<C: QueryOne>(
    conn: &C,
    release_id: &str,
    original_filename: &str,
) -> Result<String, DbError> {
    resolve_release_path(conn, release_id, |album_id, release_id, source_folder| {
        crate::storage::readable_path::audio_key(
            album_id,
            release_id,
            source_folder,
            original_filename,
        )
    })
}

/// The `cloud_path` for a cover image on a browsable home:
/// `{album_id}/{release_id}/cover-{blob_id}.{ext}` (relative to the `covers`
/// namespace coven prepends). The cover row's id is its release id; the key carries
/// the row's blob id so a replaced cover writes a new object. Covers are bae's own
/// art, not part of the imported folder, so they carry no `{source_folder}` level.
pub(super) fn resolve_cover_cloud_path<C: QueryOne>(
    conn: &C,
    release_id: &str,
    blob_id: &str,
    content_type: &ContentType,
) -> Result<String, DbError> {
    resolve_release_path(conn, release_id, |album_id, release_id, _source_folder| {
        crate::storage::readable_path::cover_cloud_path(album_id, release_id, blob_id, content_type)
    })
}

/// The `cloud_path` for an artist image on a browsable home:
/// `{artist_id}/artist.{ext}` (relative to the `artist_images` namespace). Keyed
/// by the artist id alone, so it needs no DB lookup.
pub(super) fn resolve_artist_cloud_path(
    artist_id: &str,
    blob_id: &str,
    content_type: &ContentType,
) -> String {
    crate::storage::readable_path::artist_cloud_path(artist_id, blob_id, content_type)
}

/// Insert one row into `release_identities`. Shared by the atomic import path
/// (`finalize_import_atomic` / `set_identity_atomic`, inside a transaction) and
/// `insert_release_identities` (on the connection directly).
pub(super) fn insert_release_identity_row(
    conn: &SqlContext<'_, '_>,
    release_id: &str,
    identity: &crate::import::ReleaseIdentity,
    id: String,
    reg: &str,
    now: &str,
) -> Result<(), DbError> {
    conn.execute(
        r#"
        INSERT INTO release_identities (
            id, release_id, source, source_group_id, source_release_id,
            _updated_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            id,
            release_id,
            identity.source.as_str(),
            identity.source_group_id,
            identity.source_release_id,
            reg,
            now,
        ],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Replace every `album_artists` row for `album_id` with `artists` (delete then
/// insert), so the `album_artists` schema is written in one place.
pub(super) fn replace_album_artists(
    conn: &SqlContext<'_, '_>,
    album_id: &str,
    artists: &[DbAlbumArtist],
    reg: &str,
    now: &str,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM album_artists WHERE album_id = ?",
        params![album_id],
    )?;
    for aa in artists {
        conn.execute(
            r#"INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
            VALUES (?, ?, ?, ?, ?, ?)"#,
            params![aa.id, album_id, aa.artist_id, aa.position, reg, now],
        )?;
    }
    Ok(())
}

/// Replace `track_artists` rows for every id in `track_ids`, then insert the
/// new rows. Callers pass the affected track ids explicitly because `artists`
/// may not cover every track (a track legitimately has no per-track artists
/// when it inherits from the album).
pub(super) fn replace_track_artists(
    conn: &SqlContext<'_, '_>,
    track_ids: &[&str],
    artists: &[DbTrackArtist],
    reg: &str,
    now: &str,
) -> Result<(), DbError> {
    for track_id in track_ids {
        conn.execute(
            "DELETE FROM track_artists WHERE track_id = ?",
            params![track_id],
        )?;
    }
    for ta in artists {
        conn.execute(
            r#"INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
            VALUES (?, ?, ?, ?, ?, ?)"#,
            params![ta.id, ta.track_id, ta.artist_id, ta.position, reg, now],
        )?;
    }
    Ok(())
}

/// Per-track display metadata: one row per distinct track in the queue, fetched
/// once and joined onto every queue entry that plays that track. Carries no
/// identity (no entry id, no track id) — `resolve_queue_entries` supplies those
/// from the entries.
pub(super) struct TrackQueueMeta {
    pub(super) title: String,
    pub(super) artist_names: String,
    pub(super) duration_ms: Option<i64>,
    pub(super) album_title: String,
    pub(super) cover_image: Option<crate::album_detail::ImageRef>,
}

/// Join per-track metadata onto each queue entry, preserving order and duplicates.
/// A track id appears once in `meta_by_track` but resolves once per entry, so the
/// lookup is `get`, not `remove` — `remove` would consume the row on first hit and
/// silently drop later occurrences. Display rows are keyed on each entry's
/// per-instance id, not a position, which is what lets the UI target duplicate
/// tracks independently.
pub(super) fn resolve_queue_entries(
    meta_by_track: &std::collections::HashMap<String, TrackQueueMeta>,
    entries: &[QueueEntry],
) -> Vec<QueueItem> {
    entries
        .iter()
        .filter_map(|entry| {
            let Some(meta) = meta_by_track.get(&entry.track_id) else {
                // No metadata means the queue references a track no longer in the
                // library — an inconsistency, since library deletion clears the
                // track from the queue. Drop it, but surface it.
                tracing::warn!(
                    "queue entry {} references track {} with no metadata; dropping from the queue projection",
                    entry.id.0,
                    entry.track_id
                );
                return None;
            };
            Some(QueueItem {
                entry_id: entry.id.0.clone(),
                track_id: entry.track_id.clone(),
                title: meta.title.clone(),
                artist_names: meta.artist_names.clone(),
                duration_ms: meta.duration_ms,
                album_title: meta.album_title.clone(),
                cover_image: meta.cover_image.clone(),
            })
        })
        .collect()
}
