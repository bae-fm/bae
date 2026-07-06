use super::*;

impl Database {
    /// Insert a new release.
    pub async fn insert_release(&self, release: &DbRelease) -> Result<(), DbError> {
        let release = release.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_release_row(sql.connection(), &release, &reg)
        })
        .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_composition_fixture_rows(
        &self,
        works: &[DbWork],
        track_works: &[DbTrackWork],
        images: &[DbLibraryImage],
    ) -> Result<(), DbError> {
        let (works, track_works, images) = (works.to_vec(), track_works.to_vec(), images.to_vec());
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.connection();
            for work in &works {
                insert_work_row(conn, work, &reg)?;
            }
            for track_work in &track_works {
                insert_track_work_row(conn, track_work, &reg)?;
            }
            for image in &images {
                upsert_library_image_row(conn, image, &reg)?;
            }
            Ok(())
        })
        .await
    }

    /// Insert album, release, and tracks in a single transaction
    /// Note: Artists and artist relationships should be inserted separately before calling this
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[DbReleaseMetadata],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (album, release, tracks, metadata, track_artists) = (
            album.clone(),
            release.clone(),
            tracks.to_vec(),
            metadata.to_vec(),
            track_artists.to_vec(),
        );
        self.call_sql(move |sql| {
            let tx = sql.connection();
            // One HLC stamp for every synced row this transaction writes.
            let reg = sql.stamp();
            insert_album_row(tx, &album, &reg)?;
            insert_release_row(tx, &release, &reg)?;
            for track in &tracks {
                insert_track_row(tx, track, &reg)?;
            }
            for ta in &track_artists {
                insert_track_artist_row(tx, ta, &reg)?;
            }
            for meta in &metadata {
                insert_release_metadata_row(tx, meta)?;
            }
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[DbReleaseMetadata],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (release, tracks, metadata, track_artists) = (
            release.clone(),
            tracks.to_vec(),
            metadata.to_vec(),
            track_artists.to_vec(),
        );
        self.call_sql(move |sql| {
            let tx = sql.connection();
            // One HLC stamp for every synced row this transaction writes.
            let reg = sql.stamp();
            insert_release_row(tx, &release, &reg)?;
            for track in &tracks {
                insert_track_row(tx, track, &reg)?;
            }
            for ta in &track_artists {
                insert_track_artist_row(tx, ta, &reg)?;
            }
            for meta in &metadata {
                insert_release_metadata_row(tx, meta)?;
            }
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Write a user-supplied metadata edit (from the EditMetadataSheet) in a
    /// single transaction:
    ///
    /// - Updates album-level fields, release pressing fields, and track
    ///   metadata.
    /// - Replaces `album_artists` and `track_artists` rows for the affected
    ///   album/tracks.
    /// - Does NOT touch `release_metadata` rows — the cached source payload is
    ///   independent of a user edit.
    /// - Does NOT touch `release_identities`, `metadata_source`, or
    ///   `metadata_source_release_id` — identity is orthogonal to metadata.
    ///
    /// `track_updates` maps existing track IDs to their edited rows.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_release_metadata_user_edit(
        &self,
        album_id: &str,
        release_id: &str,
        album: &DbAlbum,
        release: &DbRelease,
        track_updates: &[(String, DbTrack)],
        album_artists: &[DbAlbumArtist],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (album_id, release_id, album, release, track_updates, album_artists, track_artists) = (
            album_id.to_string(),
            release_id.to_string(),
            album.clone(),
            release.clone(),
            track_updates.to_vec(),
            album_artists.to_vec(),
            track_artists.to_vec(),
        );
        let now = self.inner.clock.now().to_rfc3339();
        self.call_sql(move |sql| {
            let tx = sql.connection();
            // One HLC stamp for every synced row this edit touches.
            let reg = sql.stamp();

            // 1. Update album.
            tx.execute(
                r#"UPDATE albums SET title = ?, artist_id = ?, year = ?, is_compilation = ?,
                    _updated_at = ? WHERE id = ?"#,
                params![
                    album.title,
                    album.artist_id,
                    album.year,
                    album.is_compilation,
                    reg,
                    album_id,
                ],
            )?;

            // 2. Update release pressing fields.
            tx.execute(
                r#"UPDATE releases SET year = ?, format = ?, label = ?, catalog_number = ?,
                    country = ?, barcode = ?, _updated_at = ? WHERE id = ?"#,
                params![
                    release.pressing.year,
                    release.pressing.format,
                    release.pressing.label,
                    release.pressing.catalog_number,
                    release.pressing.country,
                    release.pressing.barcode,
                    reg,
                    release_id,
                ],
            )?;

            // 3. Update tracks by existing ID.
            for (existing_id, new_track) in &track_updates {
                tx.execute(
                    r#"UPDATE tracks SET title = ?, side = ?, track_number = ?,
                        _updated_at = ? WHERE id = ?"#,
                    params![
                        new_track.title,
                        new_track.side,
                        new_track.track_number,
                        reg,
                        existing_id,
                    ],
                )?;
            }

            // 4. Replace album_artists.
            replace_album_artists(tx, &album_id, &album_artists, &reg, &now)?;

            // 5. Replace track_artists for the affected tracks.
            let track_ids: Vec<&str> = track_updates.iter().map(|(id, _)| id.as_str()).collect();
            replace_track_artists(tx, &track_ids, &track_artists, &reg, &now)?;

            Ok(())
        })
        .await
    }

    /// The SELECT for a release's storage summary (`DbReleaseStorageSummary`).
    /// `tail` is the trailing clause that differs per caller: an `ORDER BY` for
    /// the all-releases list, a `WHERE r.id = ?1` for a single-release lookup.
    fn release_storage_summary_query(tail: &str) -> String {
        format!(
            "SELECT \
            r.id AS release_id, \
            r.album_id, \
            a.title AS album_title, \
            r.format, \
            r.remote, \
            (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) AS any_file_id, \
            COALESCE( \
                a.primary_release_id, \
                (SELECT r2.id FROM releases r2 WHERE r2.album_id = a.id ORDER BY r2.created_at LIMIT 1) \
            ) AS primary_release_id, \
            {artist_names} AS artist_names, \
            COALESCE(( \
                SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS file_count, \
            COALESCE(( \
                SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS total_size \
        FROM releases r \
        JOIN albums a ON a.id = r.album_id \
        {tail}",
            artist_names = album_artist_names_sql()
        )
    }

    pub async fn get_release_storage_summaries(
        &self,
    ) -> Result<Vec<DbReleaseStorageSummary>, DbError> {
        let query = Self::release_storage_summary_query("ORDER BY a.title, r.created_at");
        self.call(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map([], row_to_release_storage_summary)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// The storage summary for a single release, or `None` if it doesn't exist.
    /// Same shape as one row of `get_release_storage_summaries`; the download
    /// queue uses it at enqueue time to read a release's title / file count /
    /// total size for its Downloads-pane row.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseStorageSummary>, DbError> {
        let release_id = release_id.to_string();
        let query = Self::release_storage_summary_query("WHERE r.id = ?1");
        self.call(move |conn| {
            conn.query_row(&query, [release_id], row_to_release_storage_summary)
                .optional()
                .map_err(DbError::from)
        })
        .await
    }

    /// A representative file id for every remote release — one per release, or
    /// `None` for a remote release that has no files. The disconnect flow asks
    /// coven's cache whether each is pinned (kept offline) to count how many
    /// releases become unreachable when the cloud provider is removed; an unpinned
    /// remote release is reachable only through the cloud. Pin/unpin act on all a
    /// release's blobs together, so one file represents the release.
    pub async fn get_remote_release_file_ids(&self) -> Result<Vec<Option<String>>, DbError> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) \
                     FROM releases r WHERE r.remote = 1",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Count storage rows matching `filter`. Mirrors the filter logic of
    /// `get_storage_page` so `total_count` matches the filtered page's
    /// universe.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let where_clause = storage_filter_where(filter);
        let query = format!("SELECT COUNT(*) FROM releases r {where_clause}");

        self.call(move |conn| {
            conn.query_row(&query, [], |row| row.get::<_, i64>(0))
                .map(|c| c as u64)
                .map_err(DbError::from)
        })
        .await
    }

    /// Paginated storage-page query. Joins releases × albums × (optional)
    /// primary-artist sort table; both halves of the returned row are the
    /// raw aggregates the resolver maps to `ReleaseSummary` / `AlbumSummary`.
    fn storage_page_query(order_by: &str, artist_sort_join: &str, where_clause: &str) -> String {
        let album_columns = album_summary_columns();
        format!(
            "SELECT \
                r.id AS release_id, \
                r.album_id, \
                r.format AS release_format, \
                r.remote, \
                (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) AS any_file_id, \
                COALESCE(( \
                    SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = r.id \
                ), 0) AS file_count, \
                COALESCE(( \
                    SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = r.id \
                ), 0) AS total_size, \
                {album_columns} \
            FROM releases r \
            JOIN albums a ON a.id = r.album_id \
            {artist_sort_join} \
            {where_clause} \
            ORDER BY {order_by} \
            LIMIT ? OFFSET ?"
        )
    }

    pub async fn get_storage_page(
        &self,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbStorageRow>, DbError> {
        let (order_by, needs_artist_sort_join) = storage_order_by(sort);
        let artist_sort_join = album_summary_artist_join(needs_artist_sort_join);
        let where_clause = storage_filter_where(filter);

        let query = Self::storage_page_query(&order_by, artist_sort_join, where_clause);

        self.call(move |conn| {
            let mut stmt = conn.prepare(&query)?;
            let mut rows = stmt.query(params![limit as i64, offset as i64])?;
            let mut storage_rows = Vec::new();
            while let Some(row) = rows.next()? {
                let release = row_to_release_summary(row)?;
                let album = parse_album_summary_row(row)?;
                storage_rows.push(DbStorageRow { release, album });
            }
            Ok(storage_rows)
        })
        .await
    }

    /// Follow DbTrack.release_id -> DbRelease.
    /// FK navigation — row must exist. See method conventions above.
    pub async fn get_release_for_track(&self, track: &DbTrack) -> Result<DbRelease, DbError> {
        let release_id = track.release_id.clone();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM releases WHERE id = ?",
                params![release_id],
                row_to_release,
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Get the raw release-detail aggregate for a single release.
    /// `LibraryManager` resolves this into `ReleaseDetail`.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseDetail>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let Some(release) = find_release_by_id_on(conn, &release_id)? else {
                return Ok(None);
            };
            Ok(Some(build_release_detail_on(conn, release)?))
        })
        .await
    }

    /// Get all releases for an album
    pub async fn get_releases_for_album(&self, album_id: &str) -> Result<Vec<DbRelease>, DbError> {
        let album_id = album_id.to_string();
        self.call(move |conn| get_releases_for_album_on(conn, &album_id))
            .await
    }
    /// Insert a new file record
    pub async fn insert_file(&self, file: &DbFile) -> Result<(), DbError> {
        let file = file.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_file_row(sql.connection(), &file, &reg)
        })
        .await
    }

    /// Get files for a release
    pub async fn get_files_for_release(&self, release_id: &str) -> Result<Vec<DbFile>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| get_files_for_release_on(conn, &release_id))
            .await
    }
    /// Find file by ID. Caller-provided ID — may not exist.
    pub async fn find_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, DbError> {
        let file_id = file_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM release_files WHERE id = ?",
                params![file_id],
                row_to_file,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// All data needed to atomically finalize an import in a single transaction.
    /// Nothing is in the DB yet (except the import record and artists).
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_import_atomic(
        &self,
        // Album (None = existing album, already in DB)
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        metadata: &[DbReleaseMetadata],
        track_artists: &[DbTrackArtist],
        album_artists: &[DbAlbumArtist],
        works: &[DbWork],
        work_artists: &[DbWorkArtist],
        work_parts: &[DbWorkPart],
        track_works: &[DbTrackWork],
        release_artist_roles: &[DbReleaseArtistRole],
        track_artist_roles: &[DbTrackArtistRole],
        files: &[DbFile],
        audio_formats: &[DbAudioFormat],
        audio_segments: &[DbAudioSegment],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        primary_release_id: Option<(&str, &str)>, // (album_id, release_id)
        import_id: &str,
        import_status: ImportOperationStatus,
        identities: &[crate::import::ReleaseIdentity],
        // The in-place folder this import's files live in on this device. Every
        // import lands LOCAL, so each file is registered as a coven user-provided
        // external ref under this folder; a later make-Remote uploads from them
        // and drops the refs.
        local_path: &str,
        // The cloud home's storage mode, deciding the blob layout: `Opaque` keys
        // each blob by the hashed id, `Browsable` lays it out at a readable
        // `cloud_path` computed inside this transaction (ready when the gate flips).
        storage: crate::config::HomeStorage,
    ) -> Result<(), DbError> {
        let album = album.cloned();
        let release = release.clone();
        let tracks: Vec<DbTrack> = tracks_to_files
            .iter()
            .map(|tf| tf.db_track().clone())
            .collect();
        let metadata = metadata.to_vec();
        let track_artists = track_artists.to_vec();
        let album_artists = album_artists.to_vec();
        let works = works.to_vec();
        let work_artists = work_artists.to_vec();
        let work_parts = work_parts.to_vec();
        let track_works = track_works.to_vec();
        let release_artist_roles = release_artist_roles.to_vec();
        let track_artist_roles = track_artist_roles.to_vec();
        let files = files.to_vec();
        let audio_formats = audio_formats.to_vec();
        let audio_segments = audio_segments.to_vec();
        let library_image = library_image.map(|(image, bytes)| (image.clone(), bytes.to_vec()));
        let primary_release_id = primary_release_id.map(|(a, r)| (a.to_string(), r.to_string()));
        let import_id = import_id.to_string();
        let import_status = import_status.as_str().to_string();
        let identities = identities.to_vec();
        let local_path = local_path.to_string();

        let now_dt = self.inner.clock.now();
        let now = now_dt.to_rfc3339();
        let now_ts = now_dt.timestamp();
        self.inner
            .handle
            .write(move |w| {
                if let Some((image, bytes)) = &library_image {
                    w.put_blob(
                        image.image_type.namespace(),
                        image.id.clone(),
                        bytes.clone(),
                    );
                }

                w.sql(move |sql| {
                    let tx = sql.connection();
                    // Every synced row this transaction inserts shares one HLC
                    // stamp for `_updated_at`; wall-clock `now` stays
                    // for `created_at`.
                    let reg = sql.stamp();

                    // 1. Insert album (if new)
                    if let Some(album) = &album {
                        insert_album_row(tx, album, &reg)?;

                        // Album artists (only for new albums)
                        for aa in &album_artists {
                            insert_album_artist_row(tx, aa, &reg)?;
                        }
                    }

                    // 2. Insert release
                    insert_release_row(tx, &release, &reg)?;

                    // 2b. Insert per-source identity rows. Empty for Unknown
                    //     imports. `release_identities` is uniquely keyed on
                    //     `(release_id, source)`, so a release never carries two
                    //     rows for the same source.
                    for identity in &identities {
                        insert_release_identity_row(tx, &release.id, identity, &reg, &now)?;
                    }

                    // 3. Insert globally identified works before their links.
                    for work in &works {
                        insert_work_row(tx, work, &reg)?;
                    }

                    for work_artist in &work_artists {
                        insert_work_artist_row(tx, work_artist, &reg)?;
                    }

                    for work_part in &work_parts {
                        insert_work_part_row(tx, work_part, &reg)?;
                    }

                    for role in &release_artist_roles {
                        insert_release_artist_role_row(tx, role, &reg)?;
                    }

                    // 4. Insert tracks. DbTracks live inside `tracks_to_files`;
                    //    their `duration_ms` was populated by the mapper from
                    //    the CUE sheet or a standalone-file probe.
                    for track in &tracks {
                        insert_track_row(tx, track, &reg)?;
                    }

                    // 5. Insert track artists and work/role links.
                    for ta in &track_artists {
                        insert_track_artist_row(tx, ta, &reg)?;
                    }

                    for track_work in &track_works {
                        insert_track_work_row(tx, track_work, &reg)?;
                    }

                    for role in &track_artist_roles {
                        insert_track_artist_role_row(tx, role, &reg)?;
                    }

                    // 6. Insert release metadata.
                    for meta in &metadata {
                        insert_release_metadata_row(tx, meta)?;
                    }

                    // 7. Insert files, and register each as a coven
                    //    user-provided external ref (the user's own file in
                    //    place). Every import lands Local — the files ARE the
                    //    user's files at `local_path`, tracked in coven's
                    //    `local_blob_refs` so the locality-aware read serves
                    //    them and a later make-Remote uploads from them and
                    //    drops the refs. On a browsable home the readable
                    //    cloud_path is computed now (the album/release rows
                    //    exist in this tx), so it is ready when the gate flips;
                    //    an opaque home leaves it NULL (coven hashes the id).
                    //    A populated key on a Local row is harmless.
                    for file in &files {
                        let cloud_path = if storage.is_browsable() {
                            Some(resolve_audio_cloud_path(
                                tx,
                                &file.release_id,
                                &file.original_filename,
                            )?)
                        } else {
                            None
                        };
                        let file = DbFile {
                            cloud_path,
                            ..file.clone()
                        };
                        insert_file_row(tx, &file, &reg)?;
                        let path = std::path::Path::new(&local_path).join(&file.original_filename);
                        register_external_blob_on(
                            tx,
                            &file.id,
                            crate::sync::RELEASE_FILES_NAMESPACE,
                            &path,
                            file.file_size as u64,
                        )?;
                    }

                    // 8. Insert audio formats and their ordered file windows.
                    for af in &audio_formats {
                        insert_audio_format_row(tx, af, &reg)?;
                    }
                    for segment in &audio_segments {
                        insert_audio_segment_row(tx, segment, &reg)?;
                    }

                    // 9. Write the cover row and its host-provided blob in one
                    //    coven write. On a browsable home its readable cloud_path
                    //    (`{album}/{release}/cover.{ext}`) is computed now,
                    //    ready when the gate flips; an opaque home leaves it
                    //    NULL (hashed). The cover rides the release's gate, so a
                    //    Local release's cover stays private until it is made
                    //    Remote.
                    if let Some((image, _)) = &library_image {
                        let cloud_path = if storage.is_browsable() {
                            Some(resolve_cover_cloud_path(tx, &image.id, &image.content_type)?)
                        } else {
                            None
                        };
                        let image = DbLibraryImage {
                            cloud_path,
                            ..image.clone()
                        };
                        upsert_library_image_row(tx, &image, &reg)?;
                    }

                    // 10. Set album primary_release_id
                    if let Some((album_id, release_id)) = &primary_release_id {
                        tx.execute(
                            "UPDATE albums SET primary_release_id = ?, _updated_at = ? WHERE id = ?",
                            params![release_id, reg, album_id],
                        )?;
                    }

                    // 11. Link import to release and mark complete
                    tx.execute(
                        "UPDATE imports SET release_id = ?, status = ?, updated_at = ? WHERE id = ?",
                        params![release.id, import_status, now_ts, import_id,],
                    )?;

                    Ok(())
                })?;
                Ok(())
            })
            .await
            .map_err(Self::coven_error)?;
        Ok(())
    }

    /// Delete a release by ID
    ///
    /// This will cascade delete all related records:
    /// - Tracks (via FOREIGN KEY ON DELETE CASCADE)
    /// - Files (via FOREIGN KEY ON DELETE CASCADE)
    /// - Track artists, audio formats (via FOREIGN KEY ON DELETE CASCADE)
    /// - Import records referencing this release (cleared before delete)
    pub async fn delete_release(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let tx = conn;
            tx.execute(
                "UPDATE imports SET release_id = NULL WHERE release_id = ?",
                params![release_id],
            )?;
            tx.execute("DELETE FROM releases WHERE id = ?", params![release_id])?;
            Ok(())
        })
        .await
    }

    pub async fn delete_release_with_cleanup(
        &self,
        release_id: &str,
        album_id: &str,
        cleanup: DeleteCleanupPlan,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        let album_id = album_id.to_string();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.connection();
            apply_delete_cleanup_on(conn, &cleanup, &reg)?;
            conn.execute(
                "UPDATE imports SET release_id = NULL WHERE release_id = ?",
                params![release_id],
            )?;
            conn.execute("DELETE FROM releases WHERE id = ?", params![release_id])?;

            let remaining_release_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM releases WHERE album_id = ?",
                params![album_id],
                |row| row.get(0),
            )?;
            let album_deleted = remaining_release_count == 0;
            if album_deleted {
                conn.execute("DELETE FROM albums WHERE id = ?", params![album_id])?;
            } else {
                let primary_release_id: Option<String> = conn.query_row(
                    "SELECT primary_release_id FROM albums WHERE id = ?",
                    params![album_id],
                    |row| row.get(0),
                )?;
                if primary_release_id.as_deref() == Some(&release_id) {
                    conn.execute(
                        "UPDATE albums SET primary_release_id = NULL, _updated_at = ? WHERE id = ?",
                        params![reg, album_id],
                    )?;
                }
            }

            Ok(album_deleted)
        })
        .await
    }

    /// Mark an import failed and remove the release it finalized in the same DB
    /// operation. Used when remote import upload setup fails after finalize: the
    /// release was never announced to the library, and its audio files are the
    /// user's in-place source files, so this clears coven's external refs without
    /// queuing file deletion.
    pub async fn fail_import_and_delete_release(
        &self,
        import_id: &str,
        release_id: &str,
        error: &str,
    ) -> Result<(), DbError> {
        let import_id = import_id.to_string();
        let release_id = release_id.to_string();
        let error = error.to_string();
        let now = self.inner.clock.now().timestamp();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.connection();
            let album_id = conn
                .query_row(
                    "SELECT album_id FROM releases WHERE id = ?",
                    params![release_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| DbError(format!("release not found: {release_id}")))?;

            let remaining_release_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM releases WHERE album_id = ? AND id != ?",
                params![album_id, release_id],
                |row| row.get(0),
            )?;
            let primary_release_id: Option<String> = conn.query_row(
                "SELECT primary_release_id FROM albums WHERE id = ?",
                params![album_id],
                |row| row.get(0),
            )?;

            conn.execute(
                "DELETE FROM local_blob_refs
                 WHERE namespace = ?
                   AND blob_id IN (SELECT id FROM release_files WHERE release_id = ?)",
                params![crate::sync::RELEASE_FILES_NAMESPACE, release_id],
            )?;
            conn.execute(
                "UPDATE imports SET release_id = NULL WHERE release_id = ?",
                params![release_id],
            )?;
            conn.execute(
                "UPDATE imports SET status = ?, error_message = ?, updated_at = ?, release_id = NULL WHERE id = ?",
                params![
                    ImportOperationStatus::Failed.as_str(),
                    error,
                    now,
                    import_id,
                ],
            )?;
            conn.execute("DELETE FROM releases WHERE id = ?", params![release_id])?;

            if remaining_release_count == 0 {
                conn.execute("DELETE FROM albums WHERE id = ?", params![album_id])?;
            } else if primary_release_id.as_deref() == Some(&release_id) {
                conn.execute(
                    "UPDATE albums SET primary_release_id = NULL, _updated_at = ? WHERE id = ?",
                    params![reg, album_id],
                )?;
            }

            Ok(())
        })
        .await
    }

    /// Cached `release_metadata` rows for a release, keyed by `source`.
    ///
    /// Each row's `source` discriminates between the editorial source
    /// payload (`'musicbrainz'` / `'discogs'`) and supporting payloads
    /// captured at import (`'discogs_master'`, `'musicbrainz_release_group'`).
    /// `reset_metadata_to_source` reads these to replay the seeding
    /// projection without re-fetching from the network.
    pub async fn get_release_metadata_by_source(
        &self,
        release_id: &str,
    ) -> Result<HashMap<String, String>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt =
                conn.prepare("SELECT source, json FROM release_metadata WHERE release_id = ?")?;
            let rows = stmt.query_map(params![release_id], |row| {
                Ok((
                    row.get::<_, String>("source")?,
                    row.get::<_, String>("json")?,
                ))
            })?;
            rows.collect::<coven::rusqlite::Result<HashMap<_, _>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Insert a single `release_metadata` row. Used by tests to seed cached
    /// payloads without going through the full import pipeline.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release_metadata(&self, meta: &DbReleaseMetadata) -> Result<(), DbError> {
        let meta = meta.clone();
        self.call(move |conn| insert_release_metadata_row(conn, &meta))
            .await
    }
    /// Find release by ID. Caller-provided ID — may not exist.
    pub async fn find_release_by_id(&self, release_id: &str) -> Result<Option<DbRelease>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| find_release_by_id_on(conn, &release_id))
            .await
    }

    /// Test-only: flip a release's `remote` gate column directly (bumping
    /// `_updated_at`). Production flips it through coven's transitions; tests that
    /// only need a release in a given storage state set it here.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_remote_for_test(&self, release_id: &str, remote: bool) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.connection();
            conn.execute(
                "UPDATE releases SET remote = ?, _updated_at = ? WHERE id = ?",
                params![remote, reg, release_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Test-only: register each of a release's files as a coven user-provided
    /// external ref under `folder` (the in-place files of a Local release), the
    /// new-model equivalent of the removed `release_local_source` upsert. Call
    /// after the file rows are inserted.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn register_release_external_refs_for_test(
        &self,
        release_id: &str,
        folder: &str,
    ) -> Result<(), DbError> {
        let files = self.get_files_for_release(release_id).await?;
        for file in &files {
            let path = std::path::Path::new(folder).join(&file.original_filename);
            self.register_external_blob(
                &file.id,
                crate::sync::RELEASE_FILES_NAMESPACE,
                &path,
                file.file_size as u64,
            )
            .await?;
        }
        Ok(())
    }

    /// Get the release that owns a given file.
    pub async fn find_release_for_file(&self, file_id: &str) -> Result<Option<DbRelease>, DbError> {
        let file_id = file_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT r.* FROM releases r \
                     JOIN release_files rf ON rf.release_id = r.id \
                     WHERE rf.id = ?",
                params![file_id],
                row_to_release,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Count pending upload outbox entries for files belonging to a release.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM cloud_outbox co \
                     JOIN release_files rf ON rf.id = co.file_id \
                     WHERE rf.release_id = ? AND co.operation = 'upload'",
                params![release_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Check if any upload outbox entries remain for files belonging to a release.
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT 1 FROM cloud_outbox co \
                     JOIN release_files rf ON rf.id = co.file_id \
                     WHERE rf.release_id = ? AND co.operation = 'upload' \
                     LIMIT 1",
                params![release_id],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
            .map_err(DbError::from)
        })
        .await
    }

    /// Insert a new import operation record
    pub async fn insert_import(&self, import: &DbImport) -> Result<(), DbError> {
        let import = import.clone();
        self.call(move |conn| {
            conn.execute(
                r#"
                    INSERT INTO imports (
                        id, status, release_id, album_title, artist_name,
                        folder_path, created_at, updated_at, error_message
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                params![
                    import.id,
                    import.status.as_str(),
                    import.release_id,
                    import.album_title,
                    import.artist_name,
                    import.folder_path,
                    import.created_at,
                    import.updated_at,
                    import.error_message,
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Find import by ID. Caller-provided ID — may not exist.
    pub async fn find_import_by_id(&self, id: &str) -> Result<Option<DbImport>, DbError> {
        let id = id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM imports WHERE id = ?",
                params![id],
                row_to_import,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }
    /// Get all active (non-complete, non-failed) imports
    pub async fn get_active_imports(&self) -> Result<Vec<DbImport>, DbError> {
        self
            .call(move |conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT * FROM imports WHERE status IN ('preparing', 'importing') ORDER BY created_at DESC",
                    )?;
                let rows = stmt.query_map([], row_to_import)?;
                rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                    .map_err(DbError::from)
            })
            .await
    }
    /// Update import status
    pub async fn update_import_status(
        &self,
        id: &str,
        status: ImportOperationStatus,
    ) -> Result<(), DbError> {
        let id = id.to_string();
        let status = status.as_str().to_string();
        let now = self.inner.clock.now().timestamp();
        self.call(move |conn| {
            conn.execute(
                "UPDATE imports SET status = ?, updated_at = ? WHERE id = ?",
                params![status, now, id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Mark the active import linked to a release complete after its requested
    /// remote transition has actually finished.
    pub async fn complete_import_for_release(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let now = self.inner.clock.now().timestamp();
        self.call(move |conn| {
            conn.execute(
                "UPDATE imports SET status = ?, updated_at = ? WHERE release_id = ? AND status = ?",
                params![
                    ImportOperationStatus::Complete.as_str(),
                    now,
                    release_id,
                    ImportOperationStatus::Importing.as_str(),
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Update import with error message and set status to Failed
    pub async fn update_import_error(&self, id: &str, error: &str) -> Result<(), DbError> {
        let (id, error) = (id.to_string(), error.to_string());
        let now = self.inner.clock.now().timestamp();
        self.call(move |conn| {
            conn.execute(
                "UPDATE imports SET status = ?, error_message = ?, updated_at = ? WHERE id = ?",
                params![ImportOperationStatus::Failed.as_str(), error, now, id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Delete an import record from the database.
    /// Used by UI to dismiss stuck imports so they don't reappear after restart.
    pub async fn delete_import(&self, id: &str) -> Result<(), DbError> {
        let id = id.to_string();
        self.call(move |conn| {
            conn.execute("DELETE FROM imports WHERE id = ?", params![id])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }

    /// Whether a release's source folder name was already imported.
    /// Used for duplicate detection when scanning folders.
    pub async fn is_source_folder_name_imported(&self, name: &str) -> Result<bool, DbError> {
        let name = name.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT 1 FROM releases WHERE source_folder_name = ? LIMIT 1",
                params![name],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
            .map_err(DbError::from)
        })
        .await
    }

    /// Release ids whose stored `content_hash` equals `hash`. Normally zero or
    /// one (the import overwrite path keeps the hash unique), but returns all
    /// matches so a re-import sweeps any pre-existing duplicates.
    pub async fn release_ids_for_content_hash(&self, hash: &str) -> Result<Vec<String>, DbError> {
        let hash = hash.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare("SELECT id FROM releases WHERE content_hash = ?")?;
            let ids = stmt
                .query_map(params![hash], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ids)
        })
        .await
    }

    /// Whether some release in the library was imported from this exact file
    /// structure (its `content_hash` matches `hash`). The import view uses this
    /// to mark a scanned folder as already added.
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, DbError> {
        let hash = hash.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT 1 FROM releases WHERE content_hash = ? LIMIT 1",
                params![hash],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
            .map_err(DbError::from)
        })
        .await
    }
}
