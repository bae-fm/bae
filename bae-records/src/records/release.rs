use super::*;
use bae_util::timestamp::StoredTimestamp as _;

/// A release's metadata as an edit leaves it: album fields, release pressing
/// fields, the track rows named by `track_updates` (existing track ID → edited
/// row), and the album's and tracks' artist links.
///
/// Every link names its artist by the id of one of `credits`; the credits are resolved to library artists inside the write
/// and each link is written to the artist its credit resolved to.
pub struct ReleaseMetadataRows {
    pub album: DbAlbum,
    pub release: DbRelease,
    pub track_updates: Vec<(String, DbTrack)>,
    pub credits: Vec<DbArtist>,
    /// The credits that are library artists a person picked.
    pub picked: Vec<String>,
    pub album_artists: Vec<DbAlbumArtist>,
    pub track_artists: Vec<DbTrackArtist>,
}

/// Whether a release's write writes its album's fields and credits too: not
/// where the person moved the release into another album they picked, which
/// keeps its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumFields {
    Written,
    Kept,
}

/// Write `release`'s pressing over its row, its labels naming the library
/// labels `pressing` decides.
pub(super) fn update_release_pressing_on(
    tx: &CovenWrite<'_>,
    release: &DbRelease,
    pressing: &super::PressingWrite<'_>,
) -> Result<(), IdentityWriteError> {
    let facts = super::pressing_columns::FactColumns::of(&release.pressing.facts);
    tx.execute(
        r#"UPDATE releases SET year = ?,
            country = ?, region = ?, status = ?, packaging = ?,
            discogs_details = ? WHERE id = ?"#,
        params![
            release.pressing.year,
            facts.country,
            facts.region,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            release.id,
        ],
    )?;
    super::label_resolution::write_release_labels_on(
        tx,
        &release.id,
        &release.pressing.labels,
        pressing,
    )?;
    super::write::replace_release_barcodes(
        tx,
        &release.id,
        &release.pressing.barcodes,
        pressing.barcode_ids,
    )?;
    super::write::replace_release_media(
        tx,
        &release.id,
        &release.pressing.facts.media,
        pressing.media_ids,
    )?;
    Ok(())
}

/// Write `metadata`: the release's pressing fields, each named track's title
/// and those tracks' artist links, and — unless `album_fields` keeps the
/// album's own — the album's title, year, compilation flag and artist links.
/// The links are made exactly the edit's, a picked artist the library no
/// longer holds as `gone` says.
///
/// Deliberately untouched: the stored catalog release (what the source said,
/// independent of a user edit) and the release's records and where the draft
/// was read (a record is orthogonal to editable metadata).
pub(super) fn write_release_metadata_on(
    tx: &CovenWrite<'_>,
    metadata: &ReleaseMetadataRows,
    album_fields: AlbumFields,
    now: &str,
    pressing: &super::PressingWrite<'_>,
    gone: GonePicked,
) -> Result<(), IdentityWriteError> {
    let ReleaseMetadataRows {
        album,
        release,
        track_updates,
        credits,
        picked,
        album_artists,
        track_artists,
    } = metadata;

    // Decide every credit against the library as it stands now, then insert
    // new artist rows and fill empty source-ID fields on linked artists before
    // the album/track links below point at them.
    let resolved = ArtistCredits { credits, picked }.resolve_on(tx, gone)?;
    resolved.write_on(tx)?;
    let (album_artists, track_artists) =
        resolved.relinked_credit_links(credits, album_artists, track_artists);

    if album_fields == AlbumFields::Written {
        tx.execute(
            r#"UPDATE albums SET title = ?, year = ?, is_compilation = ? WHERE id = ?"#,
            params![album.title, album.year, album.is_compilation, album.id,],
        )?;
        replace_album_artists(tx, &album.id, &album_artists, now)?;
    }

    update_release_pressing_on(tx, release, pressing)?;

    for (existing_id, new_track) in track_updates {
        tx.execute(
            r#"UPDATE tracks SET title = ? WHERE id = ?"#,
            params![new_track.title, existing_id],
        )?;
    }

    let track_ids: Vec<&str> = track_updates.iter().map(|(id, _)| id.as_str()).collect();
    replace_track_artists(tx, &track_ids, &track_artists, now)?;

    Ok(())
}

impl Records {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release(&self, release: &DbRelease) -> Result<(), IdentityWriteError> {
        let release = release.clone();
        let pressing = PressingParts::new(&*self.ids, self.store.now_utc(), &release.pressing);
        self.store
            .write(move |sql| insert_release_row(&sql, &release, &pressing_write(&pressing)))
            .await
    }

    /// [`Self::insert_release`], answering how many rows the write changed,
    /// the triggers' writes included.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn rows_changed_inserting_release_for_test(
        &self,
        release: &DbRelease,
    ) -> Result<i64, IdentityWriteError> {
        let release = release.clone();
        let pressing = PressingParts::new(&*self.ids, self.store.now_utc(), &release.pressing);
        self.store
            .write(move |sql| {
                let changes = |sql: &CovenWrite<'_>| {
                    sql.query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
                };
                let before = changes(&sql)?;
                insert_release_row(&sql, &release, &pressing_write(&pressing))?;
                Ok(changes(&sql)? - before)
            })
            .await
    }

    /// Test-only: insert an album with its credits, a release, its mediums,
    /// tracks and track-artist links in one transaction. The `artists` rows
    /// the credits name must already exist — insert them first.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        album_artists: &[DbAlbumArtist],
        release: &DbRelease,
        mediums: &[DbReleaseMedium],
        tracks: &[DbTrack],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), IdentityWriteError> {
        let (album, album_artists, release, mediums, tracks, track_artists) = (
            album.clone(),
            album_artists.to_vec(),
            release.clone(),
            mediums.to_vec(),
            tracks.to_vec(),
            track_artists.to_vec(),
        );
        let pressing = PressingParts::new(&*self.ids, self.store.now_utc(), &release.pressing);
        self.store
            .write(move |sql| {
                let tx = &sql;

                insert_album_row(tx, &album)?;
                for credit in &album_artists {
                    insert_album_artist_row(tx, credit)?;
                }
                insert_release_row(tx, &release, &pressing_write(&pressing))?;
                for medium in &mediums {
                    insert_release_medium_row(tx, medium)?;
                }
                for track in &tracks {
                    insert_track_row(tx, track)?;
                }
                for ta in &track_artists {
                    insert_track_artist_row(tx, ta)?;
                }
                Ok::<(), IdentityWriteError>(())
            })
            .await?;
        Ok(())
    }

    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        mediums: &[DbReleaseMedium],
        tracks: &[DbTrack],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), IdentityWriteError> {
        let (release, mediums, tracks, track_artists) =
            (release.clone(), mediums.to_vec(), tracks.to_vec(), track_artists.to_vec());
        let pressing = PressingParts::new(&*self.ids, self.store.now_utc(), &release.pressing);
        self.store
            .write(move |sql| {
                let tx = &sql;

                insert_release_row(tx, &release, &pressing_write(&pressing))?;
                for medium in &mediums {
                    insert_release_medium_row(tx, medium)?;
                }
                for track in &tracks {
                    insert_track_row(tx, track)?;
                }
                for ta in &track_artists {
                    insert_track_artist_row(tx, ta)?;
                }
                Ok::<(), IdentityWriteError>(())
            })
            .await?;
        Ok(())
    }

    /// Write a release's metadata rows in one transaction (see
    /// `write_release_metadata_on`).
    pub async fn update_release_metadata_user_edit(
        &self,
        metadata: ReleaseMetadataRows,
    ) -> Result<(), IdentityWriteError> {
        let pressing =
            PressingParts::new(&*self.ids, self.store.now_utc(), &metadata.release.pressing);
        let now = self.store.now_utc().to_rfc3339();
        self.store
            .write(move |sql| {
                let emptiable = Emptiable::of_releases(&sql, &[metadata.release.id.as_str()])?;
                write_release_metadata_on(
                    &sql,
                    &metadata,
                    AlbumFields::Written,
                    &now,
                    &pressing_write(&pressing),
                    GonePicked::Refuse,
                )?;
                Ok(emptiable.remove_emptied(&sql)?)
            })
            .await
    }

    /// Follow `DbTrack.release_id` → `DbRelease`. FK navigation — the row must
    /// exist. See the method conventions above.
    pub async fn get_release_for_track(&self, track: &DbTrack) -> Result<DbRelease, DbError> {
        let release_id = track.release_id.clone();
        self.store
            .read(move |sql| {
                sql.query_row(&release_rows_sql("r.id = ?"), params![release_id], row_to_release)
                    .map_err(DbError::from)
            })
            .await
    }

    /// Ordered by `created_at`.
    pub async fn get_releases_for_album(&self, album_id: &str) -> Result<Vec<DbRelease>, DbError> {
        let album_id = album_id.to_string();
        self.store.read(move |sql| get_releases_for_album_on(&sql, &album_id)).await
    }
    /// Seed a release-file row for tests, its file staged from the original
    /// at `original` as a local import stages it: on this device only, with
    /// the original's path, size, modification time and hashes recorded.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_file_for_test(
        &self,
        file: &NewReleaseFile,
        original: &std::path::Path,
    ) -> Result<coven::FileId, IdentityWriteError> {
        let staged =
            self.insert_files_for_test(vec![(file.clone(), original.to_path_buf())]).await?;
        Ok(staged[0])
    }

    /// [`Self::insert_file_for_test`] for each of `files`, in one write.
    /// Several rows may name one original.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_files_for_test(
        &self,
        files: Vec<(NewReleaseFile, std::path::PathBuf)>,
    ) -> Result<Vec<coven::FileId>, IdentityWriteError> {
        self.insert_files_placed_for_test(
            files
                .into_iter()
                .map(|(file, original)| (file, original, coven::Place::DeviceOnly))
                .collect(),
        )
        .await
    }

    /// [`Self::insert_files_for_test`] with each file's place: `Cloud` queues
    /// its upload, which waits while the store has no storage.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_files_placed_for_test(
        &self,
        files: Vec<(NewReleaseFile, std::path::PathBuf, coven::Place)>,
    ) -> Result<Vec<coven::FileId>, IdentityWriteError> {
        let lease = bae_store::lease_one(self.store.test_sources().files()).await;
        let mut rows = Vec::with_capacity(files.len());
        let mut prepared = Vec::with_capacity(files.len());
        for (file, original, place) in files {
            prepared.push((
                coven::prepare_user_file(&original, &lease, |_| ()).await.map_err(DbError::from)?,
                place,
            ));
            rows.push(file);
        }
        self.store
            .write(move |sql| {
                let mut staged = Vec::with_capacity(rows.len());
                for (file, (original, place)) in rows.iter().zip(prepared) {
                    let id = sql
                        .put_user_file(crate::synced::RELEASE_FILES_NAMESPACE, original, place)
                        .map_err(DbError::from)?;
                    insert_release_file_row(&sql, file, id)?;
                    staged.push(id);
                }
                Ok(staged)
            })
            .await
    }

    pub async fn get_files_for_release(&self, release_id: &str) -> Result<Vec<DbFile>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| get_files_for_release_on(&sql, &release_id)).await
    }

    /// Insert an audio format and its segments in one transaction — the rows the
    /// import commit writes, exposed for tests that need a resolvable track
    /// without running the whole import pipeline.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_audio_format_with_segments_for_test(
        &self,
        audio_format: &DbAudioFormat,
        segments: &[DbAudioSegment],
    ) -> Result<(), DbError> {
        let (audio_format, segments) = (audio_format.clone(), segments.to_vec());
        self.store
            .call_sql(move |sql| {
                insert_audio_format_row(&sql, &audio_format)?;
                for segment in &segments {
                    insert_audio_segment_row(&sql, segment)?;
                }
                Ok(())
            })
            .await
    }

    /// Insert scrub points as the import commit writes them, for tests whose
    /// segments a test inserted.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_scrub_points_for_test(
        &self,
        points: &DbScrubPoints,
    ) -> Result<(), DbError> {
        let points = points.clone();
        self.store.call_sql(move |sql| insert_scrub_points_row(&sql, &points)).await
    }
    /// The file `file_id` of the release `release_id`, by the files' key;
    /// `None` when the release has no such file.
    pub async fn find_release_file(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<Option<DbFile>, DbError> {
        let (release_id, file_id) = (release_id.to_string(), file_id.to_string());
        self.store
            .read(move |sql| {
                sql.query_row(
                    concat!(
                        "SELECT ",
                        release_file_columns!(),
                        " FROM release_files AS f WHERE f.id = ? AND f.release_id = ?"
                    ),
                    params![file_id, release_id],
                    row_to_file,
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }

    /// Find file by ID. Caller-provided ID — may not exist.
    pub async fn find_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, DbError> {
        let file_id = file_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(
                    concat!(
                        "SELECT ",
                        release_file_columns!(),
                        " FROM release_files AS f WHERE f.id = ?"
                    ),
                    params![file_id],
                    row_to_file,
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }

    /// Delete a release in one write. Its album's next release in album order
    /// is then the primary; nothing else is written for that. The album, a
    /// work no track performs any more and an artist nothing credits any more
    /// are deleted in the same write (see `Emptiable`).
    /// Each coven file of the release goes with its last reference, and an
    /// upload of one this device still has queued is retired.
    pub async fn delete_release_with_cleanup(
        &self,
        plan: ReleaseDeletion,
    ) -> Result<(), DeletionError> {
        self.store
            .write(move |sql| {
                let emptiable = Emptiable::of_releases(&sql, &[plan.release_id()])?;
                plan.apply_on(&sql)?;
                sql.execute("DELETE FROM releases WHERE id = ?", params![plan.release_id()])?;
                Ok(emptiable.remove_emptied(&sql)?)
            })
            .await
    }

    /// Read what deleting `release_id` owes coven; see [`ReleaseDeletion`].
    /// Caller-provided ID — may not exist.
    pub async fn plan_release_deletion(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDeletion>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| ReleaseDeletion::plan_on(&sql, &release_id)).await
    }

    /// Find release by ID. Caller-provided ID — may not exist.
    pub async fn find_release_by_id(&self, release_id: &str) -> Result<Option<DbRelease>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| find_release_by_id_on(&sql, &release_id)).await
    }
}

/// Each track of the release `release` (an SQL expression naming its id) in
/// release order, on the mediums `source` names for it and on the files the
/// edits `edits` (an SQL expression, see
/// [`super::sheet_files::sheet_files_ctes`]) lay its sheets' lines on: its
/// side's kind and ordinal and its number there, its length, the length
/// `source` lists for it where that length disagrees with the track's own
/// (`bae_lengths_disagree`), how many audio formats it stores, how many main
/// segments they hold, and the first main segment's file id and where it
/// starts in that file, with the name and audio layout of the release file
/// of that id when the release holds one, and the CUE sheet that carves it,
/// with the sheet's name and the side its first track sits on (the first side
/// of a medium that is not played side by side, whose sides share its one
/// heading), how many of the sheet's `FILE` lines carve the release's
/// tracks that stay, the sheet's lines that play no audio, the sheet's `FILE` line it is under and
/// the audio files that line can play from (`sheet_audio_options`, see
/// [`super::sheet_files::sheet_audio_options_ctes`]). A track on a line moved to another file plays to that file's end where
/// it played to the end of the file before. [`release_edit_track_of_row`]
/// reads each row.
/// Whether the medium a `place` row of the editor's statement sits on is
/// played side by side.
#[cfg(desktop)]
pub(super) const SIDED: &str = "place.side_kind = 'sided'";

#[cfg(desktop)]
pub fn release_edit_tracks_sql(release: &str, source: MediumsSource<'_>, edits: &str) -> String {
    format!(
        "WITH {places}, \
         placed AS ( \
            SELECT place.id AS id, place.position AS position, place.number AS number, \
                   place.side_kind AS side_kind, place.side_ordinal AS side_ordinal, \
                   place.stated_length_ms AS place_stated_length_ms, \
                   FIRST_VALUE(place.medium_position) OVER {first} AS sheet_medium, \
                   FIRST_VALUE({side}) OVER {first} AS sheet_side, \
                   FIRST_VALUE({kind}) OVER {first} AS sheet_kind, \
                   FIRST_VALUE({ordinal}) OVER {first} AS sheet_ordinal \
            FROM place \
            LEFT JOIN carved_tracks sheet_carved ON sheet_carved.track_id = place.id \
            WHERE place.release_id = {release}), \
         {sheet_files}, {audio_options}, \
         main AS ( \
            SELECT s.track_id, s.file_id, s.start_sample * 1000 / s.sample_rate AS start_ms, \
                   s.end_sample IS NULL AND s.moved AS to_new_end, s.added, \
                   ROW_NUMBER() OVER (PARTITION BY s.track_id ORDER BY s.segment_index) AS n, \
                   COUNT(*) OVER (PARTITION BY s.track_id) AS main_count \
            FROM laid_segments s WHERE s.role = 'main'), \
         laid AS ( \
            SELECT t.*, p.id AS row_id, p.position AS placed_position, p.side_kind, \
               p.side_ordinal, p.number AS place_number, p.place_stated_length_ms, \
               CASE WHEN main.to_new_end \
                    THEN f.source_audio_duration_ms - main.start_ms \
                    ELSE t.duration_ms END AS laid_duration_ms, \
               (SELECT COUNT(*) FROM audio_formats af WHERE af.track_id = p.id) \
                   AS audio_format_count, \
               COALESCE(main.main_count, 0) AS main_count, main.file_id AS main_file_id, \
               main.start_ms AS main_start_ms, \
               f.id IS NOT NULL AS main_file_found, f.original_filename AS main_file_name, \
               f.source_audio_layout AS main_file_layout, \
               f.source_audio_duration_ms AS main_file_duration_ms, \
               carved.sheet_file_id, carved.file_reference AS sheet_file_reference, \
               sheet_file.original_filename AS sheet_name, p.sheet_medium, p.sheet_side, \
               p.sheet_kind, p.sheet_ordinal, \
               sheet_lines.bound_count AS sheet_line_count, \
               (SELECT json_group_array(json_object('file_reference', \
                                                    line.value ->> 'file_reference', \
                                                    'options', json(line.value ->> 'options'))) \
                FROM json_each(sheet_lines.lines) AS line \
                WHERE line.value ->> 'unbound') AS sheet_unbound_lines, \
               (SELECT line.value ->> 'options' FROM json_each(sheet_lines.lines) AS line \
                WHERE line.value ->> 'file_reference' = carved.file_reference) \
                   AS sheet_audio_options, \
               COALESCE(main.added, 0) AS is_added, \
               (SELECT j.value FROM json_each(json_extract({edits}, '$.titles')) AS j \
                WHERE j.key = p.id) AS added_title \
            FROM placed p \
            LEFT JOIN tracks t ON t.id = p.id \
            LEFT JOIN main ON main.track_id = p.id AND main.n = 1 \
            LEFT JOIN laid_files f ON f.id = main.file_id \
            LEFT JOIN (carved_tracks carved \
               JOIN release_files sheet_file ON sheet_file.id = carved.sheet_file_id) \
               ON carved.track_id = p.id \
            LEFT JOIN sheet_line_options sheet_lines \
               ON sheet_lines.sheet_file_id = carved.sheet_file_id) \
     SELECT laid.*, \
        CASE WHEN laid.place_stated_length_ms IS NOT NULL \
                  AND bae_lengths_disagree(laid.laid_duration_ms, laid.place_stated_length_ms) \
             THEN laid.place_stated_length_ms END AS listed_length_ms \
     FROM laid ORDER BY laid.placed_position",
        places =
            super::release_tracks::track_places_ctes(source, &format!("m.release_id = {release}")),
        first = "(PARTITION BY sheet_carved.sheet_file_id ORDER BY place.position)",
        side = super::release_tracks::sheet_side_sql(SIDED, "place.side"),
        kind = super::release_tracks::sheet_side_kind_sql(SIDED),
        ordinal = super::release_tracks::sheet_side_ordinal_sql(
            SIDED,
            "place.side_ordinal",
            "place.medium_position"
        ),
        sheet_files = super::sheet_files::sheet_files_ctes(release, edits),
        audio_options = super::sheet_files::sheet_audio_options_ctes(release),
    )
}

/// The tracks of the release `release` (an SQL expression over `params`
/// naming its id) as the release editor reads their audio, in release order,
/// each on the mediums `source` names for it and on the files the library
/// holds them on.
#[cfg(desktop)]
pub fn release_edit_tracks_on(
    sql: &impl coven::SqlRead,
    release: &str,
    source: MediumsSource<'_>,
    params: impl rusqlite::Params,
) -> Result<Vec<DbReleaseEditTrack>, DbError> {
    Ok(sql.query(
        &release_edit_tracks_sql(release, source, super::sheet_files::NO_SHEET_EDITS),
        params,
        release_edit_track_of_row,
    )?)
}

/// One row of [`release_edit_tracks_sql`] as the editor's draft reads it: a
/// track the library holds, or the track a save adds for an audio file.
#[cfg(desktop)]
pub fn release_edit_row_of_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DbReleaseEditRow> {
    if !row.get::<_, bool>("is_added")? {
        return release_edit_track_of_row(row).map(Box::new).map(DbReleaseEditRow::Stored);
    }
    let position = super::release_tracks::track_position_of(
        &row.get::<_, String>("side_kind")?,
        row.get("side_ordinal")?,
        row.get("place_number")?,
    )
    .map_err(|reason| bae_store::read::column_conversion_error(row, "side_kind", reason))?;
    Ok(DbReleaseEditRow::Added(DbReleaseEditAdded {
        file: super::sheet_files::AddedFile {
            file_id: row.get("row_id")?,
            name: row.get("main_file_name")?,
            typed_title: row.get("added_title")?,
        },
        duration_ms: count_column(row, "main_file_duration_ms")?,
        position,
    }))
}

/// One row of [`release_edit_tracks_sql`]: a track as the release editor
/// reads its audio.
#[cfg(desktop)]
pub fn release_edit_track_of_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DbReleaseEditTrack> {
    let main_file_id: Option<String> = row.get("main_file_id")?;
    let found: bool = row.get("main_file_found")?;
    let name: Option<String> = row.get("main_file_name")?;
    let layout: Option<String> = row.get("main_file_layout")?;
    let main_segment = main_file_id
        .map(|file_id| {
            let file = match (found, name) {
                (true, Some(name)) => Some(DbReleaseEditSourceFile {
                    name,
                    layout: layout
                        .map(|layout| {
                            super::read::source_audio_layout_value(row, "main_file_layout", &layout)
                        })
                        .transpose()?,
                }),
                (false, _) => None,
                (true, None) => {
                    return Err(bae_store::read::column_conversion_error(
                        row,
                        "main_file_name",
                        format!("release file {file_id} has no name"),
                    ))
                }
            };
            Ok(DbReleaseEditSegment { file_id, start_ms: row.get("main_start_ms")?, file })
        })
        .transpose()?;
    let sheet = row
        .get::<_, Option<String>>("sheet_file_id")?
        .map(|file_id| {
            Ok::<_, rusqlite::Error>(DbReleaseEditSheet {
                file_id,
                name: row.get("sheet_name")?,
                place: {
                    let kind: String = row.get("sheet_kind")?;
                    DbSheetSideChoice::of_columns(
                        row.get("sheet_medium")?,
                        row.get("sheet_side")?,
                        &kind,
                        row.get("sheet_ordinal")?,
                    )
                    .map_err(|reason| {
                        bae_store::read::column_conversion_error(row, "sheet_kind", reason)
                    })?
                },
                line_count: count_column(row, "sheet_line_count")?,
                file_reference: row.get("sheet_file_reference")?,
                unbound_lines: {
                    let lines: String = row.get("sheet_unbound_lines")?;
                    serde_json::from_str(&lines).map_err(|error| {
                        bae_store::read::column_conversion_error(
                            row,
                            "sheet_unbound_lines",
                            format!("{error}: {lines}"),
                        )
                    })?
                },
                audio_options: {
                    let options: String = row.get("sheet_audio_options")?;
                    serde_json::from_str(&options).map_err(|error| {
                        bae_store::read::column_conversion_error(
                            row,
                            "sheet_audio_options",
                            format!("{error}: {options}"),
                        )
                    })?
                },
            })
        })
        .transpose()?;
    let position = super::release_tracks::track_position_of(
        &row.get::<_, String>("side_kind")?,
        row.get("side_ordinal")?,
        row.get("place_number")?,
    )
    .map_err(|reason| bae_store::read::column_conversion_error(row, "side_kind", reason))?;
    let mut track = row_to_track(row)?;
    track.duration_ms = count_column(row, "laid_duration_ms")?;
    Ok(DbReleaseEditTrack {
        track,
        position,
        listed_length_ms: row.get("listed_length_ms")?,
        audio_format_count: row.get("audio_format_count")?,
        main_segment_count: row.get("main_count")?,
        main_segment,
        sheet,
    })
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
