use super::*;

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
/// labels `labels` decides.
pub(super) fn update_release_pressing_on(
    tx: &CovenWrite<'_>,
    release: &DbRelease,
    labels: &super::LabelWrite<'_>,
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
        labels,
    )?;
    super::write::replace_release_barcodes(tx, &release.id, &release.pressing.barcodes)?;
    super::write::replace_release_media(tx, &release.id, &release.pressing.facts.media)?;
    Ok(())
}

/// Write `metadata`: the release's pressing fields, each named track's title
/// and those tracks' artist links, and — unless `album_fields` keeps the
/// album's own — the album's title, year, compilation flag and artist links.
/// The links are made exactly the edit's.
///
/// Deliberately untouched: the stored catalog release (what the source said,
/// independent of a user edit) and the release's records and where the draft
/// was read (a record is orthogonal to editable metadata).
pub(super) fn write_release_metadata_on(
    tx: &CovenWrite<'_>,
    metadata: &ReleaseMetadataRows,
    album_fields: AlbumFields,
    now: &str,
    labels: &super::LabelWrite<'_>,
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
    let resolved = ArtistCredits { credits, picked }.resolve_on(tx)?;
    resolved.write_on(tx)?;
    let artist_ids = resolved.id_map(credits);
    let album_artists = relink_album_artists(album_artists, &artist_ids);
    let track_artists = relink_track_artists(track_artists, &artist_ids);

    if album_fields == AlbumFields::Written {
        tx.execute(
            r#"UPDATE albums SET title = ?, year = ?, is_compilation = ? WHERE id = ?"#,
            params![album.title, album.year, album.is_compilation, album.id,],
        )?;
        replace_album_artists(tx, &album.id, &album_artists, now)?;
    }

    update_release_pressing_on(tx, release, labels)?;

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
        let labels = LabelParts::new(&*self.ids, self.store.now_utc(), &release.pressing.labels);
        artist_resolution::write_resolving_identities(&self.store, move |sql| {
            insert_release_row(&sql, &release, &label_write(&labels))
        })
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
        let labels = LabelParts::new(&*self.ids, self.store.now_utc(), &release.pressing.labels);
        artist_resolution::write_resolving_identities(&self.store, move |sql| {
            let changes = |sql: &CovenWrite<'_>| {
                sql.query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
            };
            let before = changes(&sql)?;
            insert_release_row(&sql, &release, &label_write(&labels))?;
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
        let labels = LabelParts::new(&*self.ids, self.store.now_utc(), &release.pressing.labels);
        artist_resolution::write_resolving_identities(&self.store, move |sql| {
            let tx = &sql;

            insert_album_row(tx, &album)?;
            for credit in &album_artists {
                insert_album_artist_row(tx, credit)?;
            }
            insert_release_row(tx, &release, &label_write(&labels))?;
            for medium in &mediums {
                insert_release_medium_row(tx, medium)?;
            }
            for track in &tracks {
                insert_track_row(tx, track)?;
            }
            for ta in &track_artists {
                insert_track_artist_row(tx, ta)?;
            }
            Ok(())
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
        let labels = LabelParts::new(&*self.ids, self.store.now_utc(), &release.pressing.labels);
        artist_resolution::write_resolving_identities(&self.store, move |sql| {
            let tx = &sql;

            insert_release_row(tx, &release, &label_write(&labels))?;
            for medium in &mediums {
                insert_release_medium_row(tx, medium)?;
            }
            for track in &tracks {
                insert_track_row(tx, track)?;
            }
            for ta in &track_artists {
                insert_track_artist_row(tx, ta)?;
            }
            Ok(())
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
        let labels =
            LabelParts::new(&*self.ids, self.store.now_utc(), &metadata.release.pressing.labels);
        let now = self.store.now_utc().to_rfc3339();
        artist_resolution::write_resolving_identities(&self.store, move |sql| {
            let emptiable = Emptiable::of_releases(&sql, &[metadata.release.id.as_str()])?;
            write_release_metadata_on(
                &sql,
                &metadata,
                AlbumFields::Written,
                &now,
                &label_write(&labels),
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
        let mut rows = Vec::with_capacity(files.len());
        let mut prepared = Vec::with_capacity(files.len());
        for (file, original) in files {
            prepared
                .push(coven::prepare_user_file(&original, |_| ()).await.map_err(DbError::from)?);
            rows.push(file);
        }
        artist_resolution::write_resolving_identities(&self.store, move |sql| {
            let mut staged = Vec::with_capacity(rows.len());
            for (file, original) in rows.iter().zip(prepared) {
                let id = sql
                    .put_user_file(
                        crate::synced::RELEASE_FILES_NAMESPACE,
                        original,
                        coven::Place::DeviceOnly,
                    )
                    .map_err(DbError::from)?;
                insert_release_file_row(&sql, file, id)?;
                staged.push(id);
            }
            Ok(staged)
        })
        .await
    }

    /// The release's files in their order, each with the path of the original
    /// coven records on this device for its file, `None` when it records
    /// none: for a release another device imported.
    pub async fn get_files_with_originals_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<(DbFile, Option<std::path::PathBuf>)>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query(
                    concat!(
                        "SELECT ",
                        release_file_columns!(),
                        ", u.path AS original_path \
                     FROM release_files AS f \
                     LEFT JOIN coven_user_files AS u ON u.file = f.file \
                     WHERE f.release_id = ?1 \
                     ORDER BY f.original_filename COLLATE natural_order, f.id"
                    ),
                    params![release_id],
                    |row| Ok((row_to_file(row)?, row.get::<_, Option<Vec<u8>>>("original_path")?)),
                )?
                .into_iter()
                .map(|(file, original)| {
                    Ok((file, original.map(bae_store::path_column::decode).transpose()?))
                })
                .collect()
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

    /// Test-only: write a stored name onto an existing row directly, the way a
    /// changeset from another device does. coven applies a pulled row straight into
    /// SQLite, so the check on the row-write never sees it — which is why export
    /// and audio detachment build each path from the names as they read them, and
    /// what these seams let a test exercise.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_original_filename_for_test(
        &self,
        file_id: &str,
        original_filename: &str,
    ) -> Result<(), DbError> {
        let file_id = file_id.to_string();
        let original_filename = original_filename.to_string();
        self.store
            .call(move |conn| {
                conn.execute(
                    "UPDATE release_files SET original_filename = ?1 WHERE id = ?2",
                    params![original_filename, file_id],
                )
                .map(|_| ())
                .map_err(DbError::from)
            })
            .await
    }

    /// Test-only. See [`set_original_filename_for_test`](Self::set_original_filename_for_test).
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_source_folder_name_for_test(
        &self,
        release_id: &str,
        source_folder_name: &str,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let source_folder_name = source_folder_name.to_string();
        self.store
            .call(move |conn| {
                conn.execute(
                    "UPDATE releases SET source_folder_name = ?1 WHERE id = ?2",
                    params![source_folder_name, release_id],
                )
                .map(|_| ())
                .map_err(DbError::from)
            })
            .await
    }
}

/// Each of a release's tracks in release order, with how many audio formats
/// it stores and its formats' segments in segment order as a JSON array, each
/// with its role and the name and stored audio layout of the release file it
/// names.
#[cfg(desktop)]
pub(super) const RELEASE_EDIT_TRACKS: &str = "SELECT t.*, \
        (SELECT COUNT(*) FROM audio_formats af WHERE af.track_id = t.id) AS audio_format_count, \
        (SELECT json_group_array(json_object( \
                    'role', s.role, \
                    'file_id', s.file_id, \
                    'found', json(CASE WHEN f.id IS NULL THEN 'false' ELSE 'true' END), \
                    'name', f.original_filename, 'layout', f.source_audio_layout) \
                ORDER BY s.segment_index) \
         FROM audio_formats af \
         JOIN audio_format_segments s ON s.audio_format_id = af.id \
         LEFT JOIN release_files f ON f.id = s.file_id \
         WHERE af.track_id = t.id) AS segments_json \
     FROM tracks t WHERE t.release_id = ?1 ORDER BY t.position";

#[cfg(desktop)]
pub fn release_edit_tracks_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbReleaseEditTrack>, DbError> {
    #[derive(serde::Deserialize)]
    struct Segment {
        role: String,
        file_id: String,
        found: bool,
        name: Option<String>,
        layout: Option<String>,
    }
    Ok(sql.query(RELEASE_EDIT_TRACKS, params![release_id], |row| {
        let segments: Vec<Segment> = bae_store::read::json_column(row, "segments_json")?;
        let segments = segments
            .into_iter()
            .map(|segment| {
                let role =
                    crate::DbAudioSegmentRole::from_db_value(&segment.role).ok_or_else(|| {
                        bae_store::read::column_conversion_error(
                            row,
                            "segments_json",
                            format!("unknown segment role {:?}", segment.role),
                        )
                    })?;
                let file = match (segment.found, segment.name) {
                    (true, Some(name)) => Some(DbReleaseEditSourceFile {
                        name,
                        layout: segment
                            .layout
                            .map(|layout| {
                                super::read::source_audio_layout_value(
                                    row,
                                    "segments_json",
                                    &layout,
                                )
                            })
                            .transpose()?,
                    }),
                    (false, _) => None,
                    (true, None) => {
                        return Err(bae_store::read::column_conversion_error(
                            row,
                            "segments_json",
                            format!("release file {} has no name", segment.file_id),
                        ))
                    }
                };
                Ok(DbReleaseEditSegment { role, file_id: segment.file_id, file })
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(DbReleaseEditTrack {
            track: row_to_track(row)?,
            audio_format_count: row.get("audio_format_count")?,
            segments,
        })
    })?)
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
