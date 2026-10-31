use super::*;

pub(super) fn find_album_by_id_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Option<DbAlbum>, DbError> {
    sql.query_row(
        r#"
            SELECT
                id, title, year,
                is_compilation,
                created_at
            FROM albums
            WHERE id = ?
            "#,
        params![album_id],
        row_to_album,
    )
    .optional()
    .map_err(DbError::from)
}

pub(super) fn get_artists_for_album_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Vec<LibraryCredit>, DbError> {
    // Each credit in list order, showing the artist it resolves to after
    // merges; a list naming one artist twice gives it twice.
    sql.query(
        &format!(
            r#"
            SELECT {columns} FROM album_artists credit
            JOIN artists artist ON artist.id = {shown}
            LEFT JOIN artist_credits credit_name
                ON credit_name.artist_id = credit.artist_id
               AND credit_name.name_key = credit.credit_key
            WHERE credit.album_id = ?1
            ORDER BY credit.position
            "#,
            columns = LIBRARY_CREDIT_COLUMNS,
            shown = shown_artist_id("credit.artist_id"),
        ),
        params![album_id],
        row_to_library_credit,
    )
    .map_err(DbError::from)
}

/// What [`row_to_library_credit`] reads, from a query joining the credit row
/// as `credit`, its shown artist as `artist` and its credit name, if any, as
/// `credit_name`.
pub(super) const LIBRARY_CREDIT_COLUMNS: &str = "artist.id AS artist_id, \
     artist.name AS artist_name, artist.sort_name AS artist_sort_name, \
     artist.discogs_artist_id AS artist_discogs_artist_id, \
     artist.musicbrainz_artist_id AS artist_musicbrainz_artist_id, \
     artist.created_at AS artist_created_at, \
     credit_name.name_key AS credit_key, credit_name.name AS credit_name, \
     credit.join_phrase AS credit_join";

pub(super) fn row_to_library_credit(row: &Row) -> rusqlite::Result<LibraryCredit> {
    Ok(LibraryCredit {
        artist: row_to_artist_with_prefix(row, "artist_")?,
        credit: match row.get::<_, Option<String>>("credit_key")? {
            Some(key) => Some(crate::credits::CreditName {
                key,
                name: row.get("credit_name")?,
            }),
            None => None,
        },
        join: row.get("credit_join")?,
    })
}

/// One credit as [`super::query::album_credits_json_sql`] spells it.
#[derive(serde::Deserialize)]
struct CreditJson {
    id: String,
    name: String,
    sort_name: Option<String>,
    discogs_artist_id: Option<String>,
    musicbrainz_artist_id: Option<String>,
    created_at: String,
    credit_key: Option<String>,
    credit_name: Option<String>,
    join: Option<String>,
}

/// The credits a JSON column of [`super::query::album_credits_json_sql`] or
/// [`super::query::track_credits_json_sql`] holds, in list order.
pub(super) fn credits_from_json(json: &str) -> Result<Vec<LibraryCredit>, DbError> {
    let credits: Vec<CreditJson> = serde_json::from_str(json)
        .map_err(|error| DbError::Message(format!("credit list {json:?}: {error}")))?;
    credits
        .into_iter()
        .map(|credit| {
            let created_at = DateTime::parse_from_rfc3339(&credit.created_at)
                .map_err(|error| {
                    DbError::Message(format!(
                        "artist {} created_at {:?}: {error}",
                        credit.id, credit.created_at
                    ))
                })?
                .with_timezone(&Utc);
            Ok(LibraryCredit {
                artist: DbArtist {
                    id: credit.id,
                    name: credit.name,
                    sort_name: credit.sort_name,
                    discogs_artist_id: credit.discogs_artist_id,
                    musicbrainz_artist_id: credit.musicbrainz_artist_id,
                    created_at,
                },
                credit: match (credit.credit_key, credit.credit_name) {
                    (Some(key), Some(name)) => Some(crate::credits::CreditName { key, name }),
                    _ => None,
                },
                join: credit.join,
            })
        })
        .collect()
}

/// The line a credit-list JSON column prints, read in a row closure.
pub(super) fn credit_line_column(row: &Row, column: &str) -> rusqlite::Result<String> {
    let json: String = row.get(column)?;
    credits_from_json(&json)
        .map(|credits| crate::credits::library_line(&credits))
        .map_err(|error| column_conversion_error(row, column, error.to_string()))
}

pub(super) fn find_release_by_id_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbRelease>, DbError> {
    sql.query_row(
        "SELECT * FROM releases WHERE id = ?",
        params![release_id],
        row_to_release,
    )
    .optional()
    .map_err(DbError::from)
}

pub(super) fn get_releases_for_album_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Vec<DbRelease>, DbError> {
    sql.query(
        "SELECT * FROM releases WHERE album_id = ? ORDER BY album_order, id",
        params![album_id],
        row_to_release,
    )
    .map_err(DbError::from)
}

pub(super) fn build_release_detail_on(
    sql: &SqlReadContext<'_>,
    release: DbRelease,
) -> Result<ReleaseDetailRows, DbError> {
    let tracks = get_tracks_with_artists_for_release_on(sql, &release.id)?;
    let files = get_files_for_release_on(sql, &release.id)?;
    let records = get_release_records_on(sql, &release.id)?;
    let unstored_audio = unstored_audio_on(sql, &release.id)?;
    let mediums = release_mediums_on(sql, &release.id)?;
    let (pinned, remote) = sql.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM release_listing listing \
                            WHERE listing.release_id = ?1 AND {RELEASE_LISTING_PINNED}), \
                    EXISTS (SELECT 1 FROM release_listing listing \
                            WHERE listing.release_id = ?1 AND listing.stored = 1)"
        ),
        params![release.id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let download = super::release_queues::download_phase_on(sql, &release.id)?;
    let cover_version = sql.query_row(
        &format!("SELECT {}", super::files::cover_version_sql("?1")),
        params![release.id],
        |row| row.get(0),
    )?;
    let upload = super::queued_uploads::release_upload_on(sql, &release.id)?;

    Ok(ReleaseDetailRows {
        release,
        mediums,
        tracks,
        files,
        records,
        unstored_audio,
        pinned,
        remote,
        upload,
        download,
        cover_version,
    })
}

/// What coven records this device holding for each of a release's audio
/// files on one device only: the original's path with the size and
/// modification time coven recorded for it, from `coven_user_files`, which
/// lists exactly this device's originals, when the file is on this device;
/// otherwise the device it is on, from `coven_devices`, and whether that
/// device or its person was removed from the library. The disk is not read
/// here: a read holds one of the database's readers, and the disk an original
/// sits on can be slow to answer.
pub(super) fn unstored_audio_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbUnstoredAudio>, DbError> {
    let rows: Vec<UnstoredRow> = sql.query(
        "SELECT f.content_type, f.file, u.path, u.size, u.modified_seconds, u.modified_nanos, \
                d.is_self, d.name, d.state, m.removed \
         FROM release_files f JOIN coven_files coven ON coven.id = f.file \
         LEFT JOIN coven_user_files u ON u.file = f.file \
         LEFT JOIN coven_devices d ON d.device = coven_file_device(coven.coordinates) \
         LEFT JOIN coven_members m ON m.member = d.member \
         WHERE f.release_id = ? AND coven_file_device(coven.coordinates) IS NOT NULL \
         ORDER BY f.id",
        params![release_id],
        |row| {
            Ok(UnstoredRow {
                content_type: row.get(0)?,
                file: row.get(1)?,
                original: (row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?),
                is_self: row.get(6)?,
                name: row.get(7)?,
                state: row.get(8)?,
                member_removed: row.get(9)?,
            })
        },
    )?;
    rows.into_iter()
        .filter(|row| ContentType::from_mime(&row.content_type).is_audio())
        .map(UnstoredRow::into_unstored_audio)
        .collect()
}

/// One release file on one device only, as [`unstored_audio_on`] reads it.
struct UnstoredRow {
    content_type: String,
    file: String,
    /// The original's path bytes, size and modification time, when
    /// `coven_user_files` lists it.
    original: (Option<Vec<u8>>, Option<i64>, Option<i64>, Option<u32>),
    /// Whether the device the file is on is this one; `None` when
    /// `coven_devices` lists no such device, as for this device until the
    /// library connects to storage.
    is_self: Option<bool>,
    name: Option<String>,
    state: Option<String>,
    member_removed: Option<bool>,
}

impl UnstoredRow {
    /// Coven lists this device's own originals, keeps every device's
    /// registration for good, and a file is on another device only once that
    /// device, registered, wrote it: a file on this device with no original,
    /// an original for a file on another device, or another device with no
    /// registration is coven's own state broken, and the read fails naming
    /// the file.
    fn into_unstored_audio(self) -> Result<DbUnstoredAudio, DbError> {
        let broken = |what: &str| {
            DbError::Message(format!(
                "coven lists release file {} on one device only, {what}",
                self.file
            ))
        };
        let recorded = match self.original.clone() {
            (Some(path), Some(size), Some(modified_seconds), Some(modified_nanos)) => {
                Some(RecordedOriginal {
                    path: path_column::decode(path)?,
                    size,
                    modified_seconds,
                    modified_nanos,
                })
            }
            (None, None, None, None) => None,
            partial => {
                return Err(DbError::Message(format!(
                    "coven_user_files lists the original of release file {} as {partial:?}",
                    self.file
                )))
            }
        };
        // This device has no registration until the library connects to
        // storage, so its own file can be on a device `coven_devices` doesn't
        // list; another device's never is.
        match (recorded, self.is_self) {
            (Some(_), Some(false)) => {
                Err(broken("on another device, with an original recorded here"))
            }
            (Some(original), None | Some(true)) => Ok(DbUnstoredAudio::Original(original)),
            (None, Some(false)) => Ok(DbUnstoredAudio::Elsewhere(self.other_device()?)),
            (None, Some(true)) => Err(broken("on this device, with no original recorded")),
            (None, None) => Err(broken(
                "on another device it has no registration of, or on this device with no original recorded",
            )),
        }
    }

    /// The device the file is on, which is not this one: removed when its
    /// registration says so or its person was removed from the library.
    fn other_device(&self) -> Result<crate::album_detail::OtherDevice, DbError> {
        use crate::album_detail::OtherDevice;
        use crate::sync::membership::FormerDeviceState;
        let (Some(name), Some(state), Some(member_removed)) = (
            self.name.clone(),
            self.state.as_deref(),
            self.member_removed,
        ) else {
            return Err(DbError::Message(format!(
                "coven_devices lists the device of release file {} as {:?}, {:?}, {:?}",
                self.file, self.name, self.state, self.member_removed
            )));
        };
        Ok(match state {
            "replaced" => OtherDevice::Former {
                name,
                state: FormerDeviceState::Replaced,
            },
            "removed" => OtherDevice::Former {
                name,
                state: FormerDeviceState::Removed,
            },
            "active" if member_removed => OtherDevice::Former {
                name,
                state: FormerDeviceState::Removed,
            },
            "active" => OtherDevice::Active { name },
            other => {
                return Err(DbError::Message(format!(
                    "coven_devices lists the device of release file {} as {other}",
                    self.file
                )))
            }
        })
    }
}

/// [`unstored_audio_on`] for a row mapper reading a release's summary, its
/// failure carried as the mapper's error.
pub(super) fn unstored_audio_in_row(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> rusqlite::Result<Vec<DbUnstoredAudio>> {
    unstored_audio_on(sql, release_id)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
}

#[derive(Clone, PartialEq)]
pub(super) struct ReleaseDetailRows {
    pub(super) release: DbRelease,
    mediums: DbReleaseMediums,
    tracks: Vec<(DbTrack, Option<LibraryCredit>)>,
    files: Vec<DbFile>,
    records: Vec<crate::import::ReleaseRecord>,
    unstored_audio: Vec<DbUnstoredAudio>,
    pinned: bool,
    remote: bool,
    upload: Option<DbReleaseUpload>,
    download: Option<crate::library::DownloadPhase>,
    cover_version: Option<String>,
}

impl ReleaseDetailRows {
    pub(super) fn process(self) -> DbReleaseDetail {
        DbReleaseDetail {
            release: self.release,
            mediums: self.mediums,
            tracks: process_tracks(self.tracks),
            files: self.files,
            records: self.records,
            unstored_audio: self.unstored_audio,
            pinned: self.pinned,
            remote: self.remote,
            upload: self.upload,
            download: self.download,
            cover_version: self.cover_version,
        }
    }
}

/// One row per (track, artist) pair, so a track with several artists repeats.
/// The rows arrive grouped by track and ordered by artist position, and the fold
/// below rebuilds one entry per track from that run.
pub(super) fn get_tracks_with_artists_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<(DbTrack, Option<LibraryCredit>)>, DbError> {
    let query = format!(
        "SELECT
            track.id AS track_id,
            track.release_id AS track_release_id,
            track.title AS track_title,
            track.medium_id AS track_medium_id,
            track.side AS track_side,
            track.track_number AS track_track_number,
            track.duration_ms AS track_duration_ms,
            track.created_at AS track_created_at,
            {LIBRARY_CREDIT_COLUMNS}
         FROM tracks track
         LEFT JOIN track_artists credit ON credit.track_id = track.id
         LEFT JOIN artists artist ON artist.id = {}
         LEFT JOIN artist_credits credit_name
             ON credit_name.artist_id = credit.artist_id
            AND credit_name.name_key = credit.credit_key
         WHERE track.release_id = ?
         ORDER BY track.position, track.id, credit.position",
        shown_artist_id("credit.artist_id")
    );
    let joined = sql.query(&query, params![release_id], |row| {
        let track = row_to_track_with_prefix(row, "track_")?;
        let artist_id: Option<String> = row.get("artist_id")?;
        let credit = match artist_id {
            Some(_) => Some(row_to_library_credit(row)?),
            None => None,
        };
        Ok((track, credit))
    })?;

    Ok(joined)
}

fn process_tracks(joined: Vec<(DbTrack, Option<LibraryCredit>)>) -> Vec<DbTrackWithArtists> {
    let mut tracks: Vec<DbTrackWithArtists> = Vec::new();
    for (track, artist) in joined {
        if tracks.last().map(|last| last.track.id.as_str()) != Some(track.id.as_str()) {
            tracks.push(DbTrackWithArtists {
                track,
                artists: Vec::new(),
            });
        }
        if let Some(credit) = artist {
            tracks
                .last_mut()
                .expect("the row's track was just pushed")
                .artists
                .push(credit);
        }
    }

    tracks
}

/// A release's files in natural order of their names, case aside, the way
/// the import folder lists them, id breaking a tie: the order every list of
/// a release's files shows (detail, gallery, storage, export), read from the
/// index of each release's files in that order.
pub(super) const RELEASE_FILES_IN_ORDER: &str = concat!(
    "SELECT ",
    release_file_columns!(),
    " FROM release_files AS f WHERE f.release_id = ?1 \
     ORDER BY f.original_filename COLLATE natural_order, f.id"
);

pub(super) fn get_files_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbFile>, DbError> {
    let files = sql.query(RELEASE_FILES_IN_ORDER, params![release_id], row_to_file)?;
    Ok(files)
}

/// Every catalog's description of a release, in the order surfaces list
/// catalogs (`bae_catalog_rank`), read from the index of each release's
/// records in that order.
pub(super) const RELEASE_RECORDS_IN_ORDER: &str =
    "SELECT rr.catalog, rr.kind, rr.key, rr.album_key \
     FROM release_records rr \
     WHERE rr.release_id = ?1 \
     ORDER BY bae_catalog_rank(rr.catalog)";

/// Every catalog's description of a release, in the order surfaces list
/// catalogs.
pub(super) fn get_release_records_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<crate::import::ReleaseRecord>, DbError> {
    Ok(
        sql.query(RELEASE_RECORDS_IN_ORDER, params![release_id], |row| {
            let identity = crate::import::MetadataRef::new(
                parsed_column(row, "catalog")?,
                row.get::<_, String>("key")?,
            );
            match row.get::<_, String>("kind")?.as_str() {
                "pressing" => Ok(crate::import::ReleaseRecord::new(
                    &identity,
                    row.get("album_key")?,
                )),
                "album" => Ok(crate::import::ReleaseRecord::album(&identity)),
                kind => Err(rusqlite::Error::FromSqlConversionFailure(
                    row.as_ref().column_index("kind")?,
                    rusqlite::types::Type::Text,
                    format!("unknown record kind: {kind}").into(),
                )),
            }
        })?,
    )
}

/// Build a column-conversion error for a named column whose stored text the
/// mapper could not turn into its typed value, so a corrupt column surfaces like
/// any other bad read instead of panicking or silently mis-defaulting.
pub(super) fn column_conversion_error(row: &Row, column: &str, message: String) -> rusqlite::Error {
    // The column was just read, so its index resolves; if it somehow doesn't,
    // that lookup error is itself a faithful failure to return.
    match row.as_ref().column_index(column) {
        Ok(idx) => rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            message.into(),
        ),
        Err(e) => e,
    }
}

/// Read a named rfc3339 timestamp column, surfacing a malformed value as a
/// column-conversion error rather than panicking on the parse.
pub(super) fn rfc3339_column(row: &Row, column: &str) -> rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(column)?;
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            column_conversion_error(
                row,
                column,
                format!("{column} {raw:?} is not a valid rfc3339 timestamp: {e}"),
            )
        })
}

/// Read a named integer column that holds a count or a position, which the
/// schema keeps non-negative, surfacing a negative one as a column-conversion
/// error.
pub(super) fn count_column(row: &Row, column: &str) -> rusqlite::Result<u64> {
    let raw: i64 = row.get(column)?;
    u64::try_from(raw).map_err(|error| {
        column_conversion_error(row, column, format!("{column} {raw} is negative: {error}"))
    })
}

/// [`count_column`] for a nullable column.
pub(super) fn optional_count_column(row: &Row, column: &str) -> rusqlite::Result<Option<u64>> {
    match row.get::<_, Option<i64>>(column)? {
        None => Ok(None),
        Some(_) => count_column(row, column).map(Some),
    }
}

/// Read a named text column back into the type whose `FromStr` it was written
/// from, surfacing a word no variant answers to as a column-conversion error.
pub(super) fn parsed_column<T>(row: &Row, column: &str) -> rusqlite::Result<T>
where
    T: std::str::FromStr<Err = String>,
{
    let raw: String = row.get(column)?;
    raw.parse::<T>()
        .map_err(|e| column_conversion_error(row, column, e))
}

// ─── Entity row readers ──────────────────────────────────────────────────────
//
// A query that joins several entity tables aliases each table's columns behind
// a per-entity prefix (`artist_name`, `album_title`); a query that selects one
// table leaves them bare. Same read either way, so each entity has one reader
// taking that prefix, plus a bare-column wrapper — which is also the `fn(&Row)`
// value the single-table queries hand to `query` / `query_row`.

pub(super) fn row_to_artist_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbArtist> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbArtist {
        id: row.get(column("id").as_str())?,
        name: row.get(column("name").as_str())?,
        sort_name: row.get(column("sort_name").as_str())?,
        discogs_artist_id: row.get(column("discogs_artist_id").as_str())?,
        musicbrainz_artist_id: row.get(column("musicbrainz_artist_id").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub(super) fn row_to_album_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbAlbum> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbAlbum {
        id: row.get(column("id").as_str())?,
        title: row.get(column("title").as_str())?,
        year: row.get(column("year").as_str())?,
        is_compilation: row.get(column("is_compilation").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub(super) fn row_to_track_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbTrack> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbTrack {
        id: row.get(column("id").as_str())?,
        release_id: row.get(column("release_id").as_str())?,
        title: row.get(column("title").as_str())?,
        medium_id: row.get(column("medium_id").as_str())?,
        side: row.get(column("side").as_str())?,
        track_number: row.get(column("track_number").as_str())?,
        duration_ms: row.get(column("duration_ms").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub(super) fn row_to_release_medium(row: &Row) -> rusqlite::Result<DbReleaseMedium> {
    let medium: Option<String> = row.get("medium")?;
    let carrier = medium
        .map(|key| {
            bae_types::pressing::Medium::from_key(&key).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    format!("unknown release medium {key:?}").into(),
                )
            })
        })
        .transpose()?;
    Ok(DbReleaseMedium {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        position: row.get("position")?,
        medium: crate::mediums::ReleaseMedium {
            carrier,
            sides: row.get("side_count")?,
        },
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// A release's mediums, in order.
pub(super) fn release_mediums_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<DbReleaseMediums, DbError> {
    Ok(DbReleaseMediums(sql.query(
        "SELECT * FROM release_mediums WHERE release_id = ? ORDER BY position",
        params![release_id],
        row_to_release_medium,
    )?))
}

pub(super) fn row_to_release(row: &Row) -> rusqlite::Result<DbRelease> {
    Ok(DbRelease {
        id: row.get("id")?,
        album_id: row.get("album_id")?,
        release_name: row.get("release_name")?,
        pressing: bae_types::pressing::Pressing {
            year: row.get("year")?,
            labels: super::pressing_columns::read_labels(row, "labels")?,
            barcodes: super::pressing_columns::read_barcodes(row, "barcodes")?,
            facts: super::pressing_columns::read_facts(row, "")?,
        },
        source_folder_name: row.get("source_folder_name")?,
        album_loudness_lufs: row.get("album_loudness_lufs")?,
        album_peak_linear: row.get("album_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// A stored `source_audio_layout` value, read from the column at `index`.
pub(super) fn source_audio_layout_value(
    layout: &str,
    index: usize,
) -> rusqlite::Result<crate::album_detail::SourceAudioLayout> {
    match layout {
        "file" => Ok(crate::album_detail::SourceAudioLayout::File),
        "cue" => Ok(crate::album_detail::SourceAudioLayout::Cue),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("invalid source_audio_layout {other:?}").into(),
        )),
    }
}

pub(super) fn row_to_file(row: &Row) -> rusqlite::Result<DbFile> {
    let layout = row
        .get::<_, Option<String>>("source_audio_layout")?
        .map(|layout| source_audio_layout_value(&layout, 0))
        .transpose()?;
    let source_audio = match (
        row.get::<_, Option<String>>("source_audio_content_type")?,
        row.get::<_, Option<u64>>("source_audio_duration_ms")?,
        row.get::<_, Option<i64>>("source_audio_sample_rate_hz")?,
        row.get::<_, Option<i64>>("source_audio_bits_per_sample")?,
        row.get::<_, Option<i64>>("source_audio_bitrate_kbps")?,
        row.get::<_, Option<i64>>("source_audio_channels")?,
    ) {
        (None, None, None, None, None, None) if layout.is_none() => None,
        (
            Some(content_type),
            Some(duration_ms),
            Some(sample_rate_hz),
            bits_per_sample,
            bitrate_kbps,
            Some(channels),
        ) => Some(crate::album_detail::SourceAudioFile {
            layout,
            content_type: ContentType::from_mime(&content_type),
            duration_ms,
            format: crate::album_detail::AudioFormat {
                codec: ContentType::from_mime(&content_type)
                    .display_name()
                    .to_string(),
                sample_rate_hz,
                bits_per_sample,
                bitrate_kbps,
                channels,
            },
        }),
        columns => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("inconsistent release source-audio facts: {columns:?}").into(),
            ))
        }
    };
    Ok(DbFile {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        original_filename: row.get("original_filename")?,
        file_size: row.get("file_size")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        source_audio,
        file: row.get("file")?,
        stored: row.get("stored")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_artist(row: &Row) -> rusqlite::Result<DbArtist> {
    row_to_artist_with_prefix(row, "")
}

pub(super) fn row_to_album(row: &Row) -> rusqlite::Result<DbAlbum> {
    row_to_album_with_prefix(row, "")
}

pub(super) fn row_to_track(row: &Row) -> rusqlite::Result<DbTrack> {
    row_to_track_with_prefix(row, "")
}

/// A track's stored audio value that must be above zero and fit a `u32`: its
/// sample rate or channel count. A value that doesn't is an error naming the
/// track, never a number wrapped into a different one.
fn track_audio_count_column(
    row: &Row,
    column: &str,
    track_id: &str,
) -> rusqlite::Result<std::num::NonZeroU32> {
    let raw: i64 = row.get(column)?;
    u32::try_from(raw)
        .ok()
        .and_then(std::num::NonZeroU32::new)
        .ok_or_else(|| {
            column_conversion_error(
                row,
                column,
                format!("track {track_id} has {column} {raw}, not a count above zero"),
            )
        })
}

/// A track's stored bit depth: absent for lossy audio, else 1 to 32.
fn track_bit_depth_column(
    row: &Row,
    track_id: &str,
) -> rusqlite::Result<Option<std::num::NonZeroU32>> {
    const COLUMN: &str = "bits_per_sample";
    let Some(raw) = row.get::<_, Option<i64>>(COLUMN)? else {
        return Ok(None);
    };
    u32::try_from(raw)
        .ok()
        .filter(|bits| (1..=32).contains(bits))
        .and_then(std::num::NonZeroU32::new)
        .map(Some)
        .ok_or_else(|| {
            column_conversion_error(
                row,
                COLUMN,
                format!("track {track_id} has {COLUMN} {raw}, not a depth of 1 to 32"),
            )
        })
}

pub(super) fn row_to_audio_format(row: &Row) -> rusqlite::Result<DbAudioFormat> {
    let track_id: String = row.get("track_id")?;
    Ok(DbAudioFormat {
        id: row.get("id")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        pregap_samples: optional_count_column(row, "pregap_samples")?,
        generated_pregap_samples: optional_count_column(row, "generated_pregap_samples")?,
        sample_rate: track_audio_count_column(row, "sample_rate", &track_id)?,
        bits_per_sample: track_bit_depth_column(row, &track_id)?,
        channels: track_audio_count_column(row, "channels", &track_id)?,
        track_loudness_lufs: row.get("track_loudness_lufs")?,
        track_peak_linear: row.get("track_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
        track_id,
    })
}

pub(super) fn row_to_audio_segment(row: &Row) -> rusqlite::Result<DbAudioSegment> {
    let role_text: String = row.get("role")?;
    let role = DbAudioSegmentRole::from_db_value(&role_text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown audio segment role: {role_text}").into(),
        )
    })?;
    Ok(DbAudioSegment {
        id: row.get("id")?,
        audio_format_id: row.get("audio_format_id")?,
        segment_index: row.get("segment_index")?,
        role,
        file_id: row.get("file_id")?,
        start_sample: count_column(row, "start_sample")?,
        end_sample: optional_count_column(row, "end_sample")?,
        start_byte: optional_count_column(row, "start_byte")?,
        end_byte: optional_count_column(row, "end_byte")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_release_storage_summary(
    sql: &SqlReadContext<'_>,
    row: &Row,
) -> rusqlite::Result<DbReleaseStorageSummary> {
    let release_id: String = row.get("release_id")?;
    Ok(DbReleaseStorageSummary {
        unstored_audio: unstored_audio_in_row(sql, &release_id)?,
        release_id,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        artist_names: credit_line_column(row, "artist_names")?,
        media: super::pressing_columns::read_media(row, "media")?,
        remote: row.get("remote")?,
        pinned: row.get("pinned")?,
        upload: super::queued_uploads::release_upload_in_row(row)?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
    })
}
