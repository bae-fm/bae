use super::*;

impl Database {
    pub async fn insert_release(&self, release: &DbRelease) -> Result<(), DbError> {
        let release = release.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_release_row(&sql, &release, &reg)
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
            let conn = &sql;
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

    /// Insert album, release, tracks, and track-artist links in one
    /// transaction. The `artists` rows and the album's `album_artists` links
    /// must already exist — insert them first.
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        release: &DbRelease,
        tracks: &[DbTrack],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (album, release, tracks, track_artists) = (
            album.clone(),
            release.clone(),
            tracks.to_vec(),
            track_artists.to_vec(),
        );
        self.call_sql(move |sql| {
            let tx = &sql;
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
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        tracks: &[DbTrack],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (release, tracks, track_artists) =
            (release.clone(), tracks.to_vec(), track_artists.to_vec());
        self.call_sql(move |sql| {
            let tx = &sql;
            // One HLC stamp for every synced row this transaction writes.
            let reg = sql.stamp();
            insert_release_row(tx, &release, &reg)?;
            for track in &tracks {
                insert_track_row(tx, track, &reg)?;
            }
            for ta in &track_artists {
                insert_track_artist_row(tx, ta, &reg)?;
            }
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Write a user's metadata edit (from the EditMetadataSheet) in one
    /// transaction: album fields, release pressing fields, and the track rows named
    /// by `track_updates` (existing track ID → edited row), plus a full replace of
    /// the `album_artists` and `track_artists` links.
    ///
    /// Deliberately untouched: `source_release_payloads` (the archived provider
    /// document is what the source said, independent of a user edit) and
    /// `release_identities` / `metadata_source` / `metadata_source_release_id`
    /// (identity is orthogonal to metadata).
    #[allow(clippy::too_many_arguments)]
    pub async fn update_release_metadata_user_edit(
        &self,
        album_id: &str,
        release_id: &str,
        album: &DbAlbum,
        release: &DbRelease,
        track_updates: &[(String, DbTrack)],
        artists: &[DbArtist],
        artist_external_id_updates: &[(String, DbArtist)],
        album_artists: &[DbAlbumArtist],
        track_artists: &[DbTrackArtist],
    ) -> Result<(), DbError> {
        let (
            album_id,
            release_id,
            album,
            release,
            track_updates,
            artists,
            artist_external_id_updates,
            album_artists,
            track_artists,
        ) = (
            album_id.to_string(),
            release_id.to_string(),
            album.clone(),
            release.clone(),
            track_updates.to_vec(),
            artists.to_vec(),
            artist_external_id_updates.to_vec(),
            album_artists.to_vec(),
            track_artists.to_vec(),
        );
        let now = self.inner.clock.now().to_rfc3339();
        self.call_sql(move |sql| {
            let tx = &sql;
            // One HLC stamp for every synced row this edit touches.
            let reg = sql.stamp();

            // Insert new artist rows and fill empty source-ID fields on existing
            // artists before the album/track links below point at them.
            for artist in &artists {
                insert_artist_row(tx, artist, &reg)?;
            }
            for (artist_id, artist) in &artist_external_id_updates {
                update_artist_external_ids_row(
                    tx,
                    artist_id,
                    artist.discogs_artist_id.as_deref(),
                    artist.musicbrainz_artist_id.as_deref(),
                    artist.sort_name.as_deref(),
                    &reg,
                )?;
            }

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

            replace_album_artists(tx, &album_id, &album_artists, &reg, &now)?;

            let track_ids: Vec<&str> = track_updates.iter().map(|(id, _)| id.as_str()).collect();
            replace_track_artists(tx, &track_ids, &track_artists, &reg, &now)?;

            Ok(())
        })
        .await
    }

    /// The SELECT for one release's storage summary (`DbReleaseStorageSummary`).
    fn release_storage_summary_query() -> String {
        format!(
            "SELECT \
            r.id AS release_id, \
            r.album_id, \
            a.title AS album_title, \
            r.format, \
            r.remote, \
            (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) AS any_file_id, \
            {artist_names} AS artist_names, \
            COALESCE(( \
                SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS file_count, \
            COALESCE(( \
                SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = r.id \
            ), 0) AS total_size \
        FROM releases r \
        JOIN albums a ON a.id = r.album_id \
        WHERE r.id = ?1",
            artist_names = album_artist_names_sql()
        )
    }

    /// The storage summary for one release, or `None` if it doesn't exist. The
    /// download queue reads a release's title / file count / total size from it at
    /// enqueue time, for the Downloads-pane row.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseStorageSummary>, DbError> {
        let release_id = release_id.to_string();
        let query = Self::release_storage_summary_query();
        self.read(move |sql| {
            sql.query_row(&query, [release_id], row_to_release_storage_summary)
                .optional()
                .map_err(DbError::from)
        })
        .await
    }

    /// A representative file id for every remote release — one per release, `None`
    /// for a remote release with no files. Pin/unpin act on all a release's blobs
    /// together, so one file stands for the release. The disconnect flow asks
    /// coven's cache whether each is pinned, to count the releases that become
    /// unreachable when the cloud provider is removed: an unpinned remote release
    /// is reachable only through the cloud.
    pub async fn get_remote_release_file_ids(&self) -> Result<Vec<Option<String>>, DbError> {
        self.read(move |sql| {
            sql.query(
                "SELECT (SELECT rf.id FROM release_files rf WHERE rf.release_id = r.id LIMIT 1) \
                     FROM releases r WHERE r.remote = 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// The releases coven currently has blobs queued to upload for, or empty for
    /// any filter that does not ask about uploading.
    ///
    /// Which releases are mid-upload is coven's queue state, so the storage page
    /// reads it from there and binds the ids into its own query rather than
    /// joining across into coven's tables. Every upload carries the gated root it
    /// belongs to, which for bae is always the release.
    async fn uploading_release_ids(&self, filter: StorageFilter) -> Result<Vec<String>, DbError> {
        if filter != StorageFilter::Uploading {
            return Ok(Vec::new());
        }
        let mut ids: Vec<String> = self
            .inner
            .handle
            .queued_uploads()
            .await?
            .into_iter()
            .filter(|upload| upload.root_table == "releases")
            .map(|upload| upload.root_id)
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// Count storage rows matching `filter`, using the same filter logic as
    /// `get_storage_page` so `total_count` describes the same set the page pages.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let uploading = self.uploading_release_ids(filter).await?;
        let where_clause = storage_filter_where(filter, uploading.len());
        let query = format!("SELECT COUNT(*) FROM releases r {where_clause}");

        self.read(move |conn| {
            conn.query_row(
                &query,
                coven::rusqlite::params_from_iter(uploading.iter()),
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c as u64)
            .map_err(DbError::from)
        })
        .await
    }

    /// Sum of `total_size` over every storage row matching `filter` — the
    /// storage-manager footer's "Total:", independent of how many pages have
    /// loaded. Same filter logic as `get_storage_page` / `get_storage_count`. A
    /// release with no files contributes nothing through the inner join, matching a
    /// page row's own `COALESCE(SUM(...), 0)`.
    pub async fn get_storage_total_size(&self, filter: StorageFilter) -> Result<u64, DbError> {
        let uploading = self.uploading_release_ids(filter).await?;
        let where_clause = storage_filter_where(filter, uploading.len());
        let query = format!(
            "SELECT COALESCE(SUM(rf.file_size), 0) \
             FROM releases r JOIN release_files rf ON rf.release_id = r.id \
             {where_clause}"
        );

        self.read(move |conn| {
            conn.query_row(
                &query,
                coven::rusqlite::params_from_iter(uploading.iter()),
                |row| row.get::<_, i64>(0),
            )
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
        let uploading = self.uploading_release_ids(filter).await?;
        let where_clause = storage_filter_where(filter, uploading.len());

        let query = Self::storage_page_query(&order_by, artist_sort_join, &where_clause);

        self.read(move |sql| {
            // The uploading ids bind before the page's limit/offset, matching
            // their order in the rendered clause.
            let mut binds: Vec<Box<dyn coven::rusqlite::ToSql>> = uploading
                .iter()
                .map(|id| Box::new(id.clone()) as Box<dyn coven::rusqlite::ToSql>)
                .collect();
            binds.push(Box::new(limit as i64));
            binds.push(Box::new(offset as i64));
            sql.query(
                &query,
                coven::rusqlite::params_from_iter(binds.iter()),
                |row| {
                    let release = row_to_release_summary(row)?;
                    Ok(parse_album_summary_row(row).map(|album| DbStorageRow { release, album }))
                },
            )?
            .into_iter()
            .collect()
        })
        .await
    }

    /// Follow `DbTrack.release_id` → `DbRelease`. FK navigation — the row must
    /// exist. See the method conventions above.
    pub async fn get_release_for_track(&self, track: &DbTrack) -> Result<DbRelease, DbError> {
        let release_id = track.release_id.clone();
        self.read(move |sql| {
            sql.query_row(
                "SELECT * FROM releases WHERE id = ?",
                params![release_id],
                row_to_release,
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// The raw release-detail aggregate for a single release. `LibraryManager`
    /// resolves it into `ReleaseDetail`.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseDetail>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            let Some(release) = find_release_by_id_on(&sql, &release_id)? else {
                return Ok(None);
            };
            Ok(Some(build_release_detail_on(&sql, release)?))
        })
        .await
    }

    /// The raw release-detail aggregate, the release's album artists, and its
    /// 0-based position among its album's releases — all from one database call.
    /// `LibraryManager` resolves this into `ReleaseDetail`.
    pub async fn find_release_detail_context(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetailContext>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            let Some(release) = find_release_by_id_on(&sql, &release_id)? else {
                return Ok(None);
            };
            let album_artists = get_artists_for_album_on(&sql, &release.album_id)?;
            let releases = get_releases_for_album_on(&sql, &release.album_id)?;
            let Some(release_index) = releases.iter().position(|r| r.id == release_id) else {
                return Ok(None);
            };
            // The album's compilation flag decides each track's display artist.
            let is_compilation = find_album_by_id_on(&sql, &release.album_id)?
                .is_some_and(|album| album.is_compilation);
            Ok(Some(ReleaseDetailContext {
                detail: build_release_detail_on(&sql, release)?,
                album_artists,
                release_index,
                is_compilation,
            }))
        })
        .await
    }

    /// Ordered by `created_at`.
    pub async fn get_releases_for_album(&self, album_id: &str) -> Result<Vec<DbRelease>, DbError> {
        let album_id = album_id.to_string();
        self.read(move |sql| get_releases_for_album_on(&sql, &album_id))
            .await
    }
    pub async fn insert_file(&self, file: &DbFile) -> Result<(), DbError> {
        let file = file.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_file_row(&sql, &file, &reg)
        })
        .await
    }

    pub async fn get_files_for_release(&self, release_id: &str) -> Result<Vec<DbFile>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| get_files_for_release_on(&sql, &release_id))
            .await
    }

    /// Insert an audio format and its segments in one transaction — the rows the
    /// import commit writes, exposed for tests that need a resolvable track
    /// without running the whole import pipeline.
    #[cfg(test)]
    pub(crate) async fn insert_audio_format_with_segments_for_test(
        &self,
        audio_format: &DbAudioFormat,
        segments: &[DbAudioSegment],
    ) -> Result<(), DbError> {
        let (audio_format, segments) = (audio_format.clone(), segments.to_vec());
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_audio_format_row(&sql, &audio_format, &reg)?;
            for segment in &segments {
                insert_audio_segment_row(&sql, segment, &reg)?;
            }
            Ok(())
        })
        .await
    }
    /// Find file by ID. Caller-provided ID — may not exist.
    pub async fn find_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, DbError> {
        let file_id = file_id.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT * FROM release_files WHERE id = ?",
                params![file_id],
                row_to_file,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Finalize an import in one transaction. Nothing of it is in the DB yet except
    /// the import record — every row below lands together or not at all.
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_import_atomic(
        &self,
        // `None` when the album is an existing one, already in the DB.
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        track_artists: &[DbTrackArtist],
        album_artists: &[DbAlbumArtist],
        works: &[DbWork],
        work_artists: &[DbWorkArtist],
        work_parts: &[DbWorkPart],
        track_works: &[DbTrackWork],
        release_artist_roles: &[DbReleaseArtistRole],
        track_artist_roles: &[DbTrackArtistRole],
        artists: &[DbArtist],
        artist_external_id_updates: &[(String, DbArtist)],
        files: &[DbFile],
        audio_formats: &[DbAudioFormat],
        audio_segments: &[DbAudioSegment],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        artist_images: &[(&DbLibraryImage, &[u8])],
        primary_release_id: Option<(&str, &str)>, // (album_id, release_id)
        identities: &[crate::import::ReleaseIdentity],
        // The in-place folder this import's files live in on this device. Every
        // import lands LOCAL, so each file is registered as a coven user-provided
        // external ref under this folder; a later make-Remote uploads from them and
        // drops the refs.
        local_path: &str,
        // The cloud home's storage mode, which decides the blob layout: `Opaque`
        // keys each blob by its hashed id, `Browsable` lays it out at a readable
        // `cloud_path` computed inside this transaction, ready when the gate flips.
        storage: crate::config::HomeStorage,
        replacement_deletes: &[ImportReplacementDelete],
    ) -> Result<Vec<ImportReplacementOutcome>, DbError> {
        let album = album.cloned();
        let release = release.clone();
        let tracks: Vec<DbTrack> = tracks_to_files
            .iter()
            .map(|tf| tf.db_track().clone())
            .collect();
        let track_artists = track_artists.to_vec();
        let album_artists = album_artists.to_vec();
        let works = works.to_vec();
        let work_artists = work_artists.to_vec();
        let work_parts = work_parts.to_vec();
        let track_works = track_works.to_vec();
        let release_artist_roles = release_artist_roles.to_vec();
        let track_artist_roles = track_artist_roles.to_vec();
        let artists = artists.to_vec();
        let artist_external_id_updates = artist_external_id_updates.to_vec();
        let files = files.to_vec();
        let audio_formats = audio_formats.to_vec();
        let audio_segments = audio_segments.to_vec();
        let library_image = library_image.map(|(image, bytes)| (image.clone(), bytes.to_vec()));
        let artist_images: Vec<(DbLibraryImage, Vec<u8>)> = artist_images
            .iter()
            .map(|(image, bytes)| ((*image).clone(), (*bytes).to_vec()))
            .collect();
        // Keyed by the image's `blob_id`, not its row id: a coven blob id names one
        // immutable byte-string, and the row id (the release / artist) never moves.
        let image_blobs: Vec<(String, String, Vec<u8>)> = library_image
            .iter()
            .chain(artist_images.iter())
            .map(|(image, bytes)| {
                (
                    image.image_type.namespace().to_string(),
                    image.blob_id.clone(),
                    bytes.clone(),
                )
            })
            .collect();
        let primary_release_id = primary_release_id.map(|(a, r)| (a.to_string(), r.to_string()));
        let identities = identities.to_vec();
        let local_path = local_path.to_string();
        let replacement_deletes = replacement_deletes.to_vec();

        let now_dt = self.inner.clock.now();
        let now = now_dt.to_rfc3339();
        let ids = Arc::clone(&self.inner.ids);
        let replacement_outcomes = Arc::new(Mutex::new(Vec::new()));
        let replacement_outcomes_for_write = Arc::clone(&replacement_outcomes);
        self.inner
            .handle
            .write(
                move |w| {
                    for (namespace, id, bytes) in image_blobs {
                        w.put_blob(namespace, id, bytes);
                    }
                    Ok(())
                },
                move |sql| {
                    let tx = &sql;
                    // Every synced row this transaction inserts shares one HLC stamp
                    // for `_updated_at`; wall-clock `now` stays for `created_at`.
                    let reg = sql.stamp();

                    for replacement in &replacement_deletes {
                        apply_delete_cleanup_on(tx, &replacement.cleanup)?;
                        tx.execute(
                            "DELETE FROM releases WHERE id = ?",
                            params![replacement.release_id],
                        )?;

                        let album_deleted = cleanup_album_after_release_removal_on(
                            tx,
                            &replacement.album_id,
                            &replacement.release_id,
                            &reg,
                        )?;
                        replacement_outcomes_for_write
                            .lock()
                            .expect("replacement outcomes mutex not poisoned")
                            .push(ImportReplacementOutcome {
                                release_id: replacement.release_id.clone(),
                                album_id: replacement.album_id.clone(),
                                album_deleted,
                            });
                    }

                    // Insert new artist rows and fill empty source-ID fields on
                    // existing artists: the album/release/track links below refer
                    // to the resolved ids.
                    for artist in &artists {
                        insert_artist_row(tx, artist, &reg)?;
                    }
                    for (artist_id, artist) in &artist_external_id_updates {
                        update_artist_external_ids_row(
                            tx,
                            artist_id,
                            artist.discogs_artist_id.as_deref(),
                            artist.musicbrainz_artist_id.as_deref(),
                            artist.sort_name.as_deref(),
                            &reg,
                        )?;
                    }

                    if let Some(album) = &album {
                        insert_album_row(tx, album, &reg)?;

                        for aa in &album_artists {
                            insert_album_artist_row(tx, aa, &reg)?;
                        }
                    }

                    insert_release_row(tx, &release, &reg)?;

                    // Per-source identity rows, empty for an Unknown import.
                    // `release_identities` is uniquely keyed on `(release_id,
                    // source)`, so a release never carries two rows for one source.
                    for identity in &identities {
                        insert_release_identity_row(
                            tx,
                            &release.id,
                            identity,
                            ids.new_id(),
                            &reg,
                            &now,
                        )?;
                    }

                    // Works are globally identified; they go in before their links.
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

                    // The tracks came out of `tracks_to_files`; their `duration_ms`
                    // was set by the mapper, from the CUE sheet or a standalone-file
                    // probe.
                    for track in &tracks {
                        insert_track_row(tx, track, &reg)?;
                    }

                    for ta in &track_artists {
                        insert_track_artist_row(tx, ta, &reg)?;
                    }

                    for track_work in &track_works {
                        insert_track_work_row(tx, track_work, &reg)?;
                    }

                    for role in &track_artist_roles {
                        insert_track_artist_role_row(tx, role, &reg)?;
                    }

                    // Each file is also registered as a coven user-provided external
                    // ref. Every import lands Local — the files ARE the user's own
                    // files at `local_path`, registered with coven so its
                    // locality-aware read serves them, and a later make-Remote uploads
                    // from them and drops the refs. On a browsable home the readable
                    // cloud_path is computed here (the album/release rows exist in
                    // this tx) so it is ready when the gate flips; an opaque home
                    // leaves it NULL and coven hashes the id. A populated key on a
                    // Local row is harmless.
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
                        // Register the user's own file as this row's external
                        // blob, after the row exists: coven binds the exact row
                        // reference (size and content hash included) and serves
                        // the plaintext from this path while the release is
                        // Local. `release_files` is the only user-provided table
                        // — the image tables are host-provided, and coven refuses
                        // an external registration on those.
                        let path = std::path::Path::new(&local_path).join(&file.original_filename);
                        tx.register_external_blob("release_files", &file.id, &path)?;
                    }

                    for af in &audio_formats {
                        insert_audio_format_row(tx, af, &reg)?;
                    }
                    for segment in &audio_segments {
                        insert_audio_segment_row(tx, segment, &reg)?;
                    }

                    // The cover row; its blob went into the same coven write above.
                    // On a browsable home the readable cloud_path
                    // (`{album}/{release}/cover-{blob_id}.{ext}`) is computed here,
                    // ready when the gate flips; an opaque home leaves it NULL
                    // (hashed). The cover rides the release's gate, so a Local
                    // release's cover stays private until the release is made Remote.
                    if let Some((image, _)) = &library_image {
                        let cloud_path = if storage.is_browsable() {
                            Some(resolve_cover_cloud_path(
                                tx,
                                &image.id,
                                &image.blob_id,
                                &image.content_type,
                            )?)
                        } else {
                            None
                        };
                        upsert_library_image_row_with_cloud_path(tx, image, cloud_path, &reg)?;
                    }

                    // Artist image rows; their blobs went into the same batch above.
                    for (image, _) in &artist_images {
                        let cloud_path = artist_image_cloud_path_for_storage(
                            storage,
                            &image.id,
                            &image.blob_id,
                            &image.content_type,
                        );
                        upsert_library_image_row_with_cloud_path(tx, image, cloud_path, &reg)?;
                    }

                    if let Some((album_id, release_id)) = &primary_release_id {
                        tx.execute(
                            "UPDATE albums SET primary_release_id = ?, _updated_at = ? WHERE id = ?",
                            params![release_id, reg, album_id],
                        )?;
                    }


                    Ok(())
                },
            )
            .await
            .map_err(Self::coven_error)?;
        Ok(Arc::try_unwrap(replacement_outcomes)
            .expect("replacement outcomes captured only by finalize_import_atomic")
            .into_inner()
            .expect("replacement outcomes mutex not poisoned"))
    }

    /// Delete a release row, apply its cleanup plan, and remove the album when this
    /// was its last release. Deliberately does NOT sweep now-orphaned artists,
    /// works, work_parts, or artist-image blobs — retaining them is a sync-safety
    /// invariant.
    ///
    /// This delete may target a remote (sync-visible) release, and artist/work rows
    /// are shared across devices (import find-or-create reuses them by
    /// Discogs/MusicBrainz id or name). Deleting a locally-orphaned artist or work
    /// row here would emit that DELETE to peers, and a peer that concurrently
    /// imported a release referencing the same row either cascade-loses its link
    /// rows (the link tables are ON DELETE CASCADE) or hits a foreign-key violation
    /// (`albums.artist_id` has no delete action) that rolls back the whole incoming
    /// changeset and holds that device's pull cursor forever. The deleting device
    /// wedges the same way in reverse when the peer's link-row INSERTs referencing
    /// the deleted row arrive.
    ///
    /// Orphaned rows are inert instead: unreachable from the UI (artists and works
    /// surface only through albums, tracks, and work links), and cut from outgoing
    /// changesets and bootstrap snapshots by coven's descendant gate, so they never
    /// reach new devices and are re-referenced in place if a later import matches
    /// the same artist or work.
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
            let conn = &sql;
            apply_delete_cleanup_on(conn, &cleanup)?;
            conn.execute("DELETE FROM releases WHERE id = ?", params![release_id])?;

            let album_deleted =
                cleanup_album_after_release_removal_on(conn, &album_id, &release_id, &reg)?;

            Ok(album_deleted)
        })
        .await
    }

    /// Mark an import failed and remove the release it finalized. Used when
    /// remote-import upload setup fails after finalize: the release was never
    /// announced to the library, and its audio files are the user's in-place
    /// source files, so this clears coven's external refs without queuing any file
    /// deletion.
    ///
    /// That is also why this path MAY sweep the orphaned artists/works it finds
    /// where `delete_release_with_cleanup` must not: the rolled-back release was
    /// never remote, so the swept rows had no kept descendants and coven's outbound
    /// gate cuts their DELETEs from the changeset — the sweep never leaves this
    /// device.
    ///
    /// The rollback drops the cover/artist-image rows, and the host-provided image
    /// blobs those rows carried must be reclaimed from coven's on-device store. A
    /// bare row DELETE reclaims nothing — coven's local-blob cleanup is intent-
    /// driven, and an intent is created only for a blob **declared** deleted in a
    /// write batch. So this reads the exact delete set and its blobs first
    /// (`plan_fail_import_deletion`), then in one atomic [`CovenHandle::write`]
    /// declares those blob deletions and deletes the rows: the blobs are referenced
    /// when the batch opens (so coven binds a row cleanup intent) and unreferenced
    /// when it commits (so the intent records), reclaiming the local bytes.
    ///
    /// The plan is read on the same serialized writer path immediately before the
    /// write, so the rows it names to delete are exactly the rows the write
    /// deletes; a divergence surfaces as coven's `BlobStillReferenced` (a declared
    /// blob whose row wasn't dropped) rather than a silent leak.
    pub async fn fail_import_and_delete_release(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let plan = self
            .read(move |sql| plan_fail_import_deletion(&sql, &release_id))
            .await?;
        let FailImportDeletion {
            release_id,
            album_id,
            delete_album,
            orphaned_work_ids,
            orphaned_artist_ids,
            image_blobs,
            file_ids,
        } = plan;
        let declared = image_blobs.clone();
        self.inner
            .handle
            .write(
                move |w| {
                    for (namespace, blob_id, cloud_path) in declared {
                        w.delete_blob(crate::sync::image_blob_ref(namespace, &blob_id, cloud_path));
                    }
                    Ok(())
                },
                move |sql| {
                    let reg = sql.stamp();
                    // Drop each file's external-file registration while its row is
                    // still there to bind: the rollback leaves the user's own
                    // source files alone, it only stops coven pointing at them.
                    for file_id in &file_ids {
                        sql.clear_external_blob("release_files", file_id)
                            .map_err(CovenError::from)?;
                    }
                    // Dropping the release cascades its tracks, links, and `covers`
                    // row; the declared cover blob is unreferenced once it commits.
                    sql.execute("DELETE FROM releases WHERE id = ?", params![release_id])
                        .map_err(CovenError::from)?;
                    if delete_album {
                        sql.execute("DELETE FROM albums WHERE id = ?", params![album_id])
                            .map_err(CovenError::from)?;
                    } else {
                        // The album keeps other releases; only clear a
                        // `primary_release_id` that pointed at the removed release.
                        sql.execute(
                            "UPDATE albums SET primary_release_id = NULL, _updated_at = ? \
                             WHERE id = ? AND primary_release_id = ?",
                            params![reg, album_id, release_id],
                        )
                        .map_err(CovenError::from)?;
                    }
                    // Delete the planned orphaned works and artists by id: the plan
                    // determined each is referenced only within this deleted
                    // subtree, so removing them strands nothing. Each artist delete
                    // cascades its `artist_images` row, freeing the declared blob.
                    for work_id in &orphaned_work_ids {
                        sql.execute("DELETE FROM works WHERE id = ?", params![work_id])
                            .map_err(CovenError::from)?;
                    }
                    for artist_id in &orphaned_artist_ids {
                        sql.execute("DELETE FROM artists WHERE id = ?", params![artist_id])
                            .map_err(CovenError::from)?;
                    }
                    Ok(())
                },
            )
            .await
            .map(|_| ())
            .map_err(Self::coven_error)
    }

    /// Find release by ID. Caller-provided ID — may not exist.
    pub async fn find_release_by_id(&self, release_id: &str) -> Result<Option<DbRelease>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| find_release_by_id_on(&sql, &release_id))
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
            let conn = &sql;
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
    /// external ref under `folder` — the in-place files of a Local release. Call
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
            self.register_external_blob(crate::sync::RELEASE_FILES_NAMESPACE, &file.id, &path)
                .await?;
        }
        Ok(())
    }

    /// Test-only: write a path fragment onto an existing row directly, the way a
    /// changeset from another device does. coven applies a pulled row straight into
    /// SQLite, so the validation on the row-write never sees it — which is why
    /// export and make-Local validate the fragment again before joining it onto a
    /// local directory, and what these seams let a test exercise.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_original_filename_for_test(
        &self,
        file_id: &str,
        original_filename: &str,
    ) -> Result<(), DbError> {
        let file_id = file_id.to_string();
        let original_filename = original_filename.to_string();
        self.call(move |conn| {
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
        self.call(move |conn| {
            conn.execute(
                "UPDATE releases SET source_folder_name = ?1 WHERE id = ?2",
                params![source_folder_name, release_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// How many of this release's blobs are still queued to upload.
    ///
    /// The release is the gated root every one of its uploads is enqueued under,
    /// so coven filters them in SQL rather than bae joining its own tables to
    /// coven's queue. Note this reaches zero before the make-remote transition
    /// finishes — see [`CovenHandle::make_remote_progress`] for that.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        Ok(self
            .inner
            .handle
            .queued_uploads_for_root("releases", release_id)
            .await?
            .len() as i64)
    }

    /// Whether any of this release's blobs are still queued to upload. See
    /// [`count_pending_uploads_for_release`](Self::count_pending_uploads_for_release).
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        Ok(!self
            .inner
            .handle
            .queued_uploads_for_root("releases", release_id)
            .await?
            .is_empty())
    }

    /// How far this release's make-Remote has got, or `None` when it has none
    /// running. The transition outlasts its queued uploads — a release with an
    /// empty upload queue can still be `Publishing` — so this, not
    /// [`has_pending_uploads_for_release`](Self::has_pending_uploads_for_release),
    /// answers "is this release still becoming Remote?".
    pub async fn make_remote_progress_for_release(
        &self,
        release_id: &str,
    ) -> Result<Option<coven::MakeRemoteProgress>, DbError> {
        self.inner
            .handle
            .make_remote_progress("releases", release_id)
            .await
    }

    /// Release ids whose stored `content_hash` equals `hash`. Normally zero or
    /// one (the import overwrite path keeps the hash unique), but returns all
    /// matches so a re-import sweeps any pre-existing duplicates.
    pub async fn release_ids_for_content_hash(&self, hash: &str) -> Result<Vec<String>, DbError> {
        let hash = hash.to_string();
        self.read(move |sql| {
            sql.query(
                "SELECT id FROM releases WHERE content_hash = ?",
                params![hash],
                |row| row.get::<_, String>(0),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Whether some release in the library was imported from this exact file
    /// structure (its `content_hash` matches `hash`). The import view uses this
    /// to mark a scanned folder as already added.
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, DbError> {
        let hash = hash.to_string();
        self.read(move |sql| {
            sql.query_row(
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

    pub async fn imported_content_hashes(
        &self,
    ) -> Result<std::collections::HashSet<String>, DbError> {
        self.read(move |sql| {
            Ok(sql
                .query(
                    "SELECT DISTINCT content_hash FROM releases WHERE content_hash IS NOT NULL",
                    [],
                    |row| row.get::<_, String>(0),
                )?
                .into_iter()
                .collect())
        })
        .await
    }
}

/// The exact rows [`Database::fail_import_and_delete_release`] deletes for a
/// rolled-back release, plus the host-provided image blobs those rows carry, so
/// the atomic write that deletes them can declare the blob deletions coven turns
/// into durable local-cleanup intents.
struct FailImportDeletion {
    release_id: String,
    album_id: String,
    /// The album is removed with the release iff it holds no other release.
    delete_album: bool,
    /// Works whose only tracks were this release's — unreferenced once it is gone.
    orphaned_work_ids: Vec<String>,
    /// Artists referenced only within this release's deleted subtree.
    orphaned_artist_ids: Vec<String>,
    /// `(namespace, blob_id, cloud_path)` for the release's cover and each swept
    /// artist's image — the host-provided blobs the delete orphans.
    image_blobs: Vec<(&'static str, String, Option<String>)>,
    /// The release's `release_files` rows, whose external-file registrations the
    /// delete must drop. Captured here because the registration is keyed by the
    /// row, and the rows are gone by the time the write commits.
    file_ids: Vec<String>,
}

/// Determine, without mutating, exactly what a failed-import rollback of
/// `release_id` deletes and which host-provided image blobs it orphans. Mirrors
/// the delete's reachability: a work orphans when this release held its only
/// tracks; an artist orphans when every album, link, or role that names it lies
/// inside the deleted subtree — this release, its album if the album holds no
/// other release, and the orphaned works.
fn plan_fail_import_deletion(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<FailImportDeletion, DbError> {
    let album_id: String = sql
        .query_row(
            "SELECT album_id FROM releases WHERE id = ?",
            params![release_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DbError::Message(format!("release not found: {release_id}")))?;

    let delete_album: bool = sql.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM releases WHERE album_id = ?1 AND id != ?2)",
        params![album_id, release_id],
        |row| row.get(0),
    )?;

    // Every artist this release/album references — the candidates the sweep
    // considers.
    let mut candidate_artist_ids: Vec<String> = sql.query(
        "SELECT artist_id FROM album_artists WHERE album_id = ?1
             UNION
             SELECT artist_id FROM track_artists
               WHERE track_id IN (SELECT id FROM tracks WHERE release_id = ?2)
             UNION
             SELECT artist_id FROM release_artist_roles WHERE release_id = ?2
             UNION
             SELECT artist_id FROM track_artist_roles
               WHERE track_id IN (SELECT id FROM tracks WHERE release_id = ?2)
             UNION
             SELECT artist_id FROM albums WHERE id = ?1 AND artist_id IS NOT NULL",
        params![album_id, release_id],
        |row| row.get(0),
    )?;

    // The work-graph component this release's tracks touch.
    let candidate_work_ids: Vec<String> = sql.query(
        "WITH RECURSIVE component(work_id) AS (
                 SELECT tw.work_id FROM track_works tw
                   JOIN tracks t ON t.id = tw.track_id
                  WHERE t.release_id = ?1
                 UNION
                 SELECT wp.parent_work_id FROM work_parts wp
                   JOIN component c ON wp.child_work_id = c.work_id
                 UNION
                 SELECT wp.child_work_id FROM work_parts wp
                   JOIN component c ON wp.parent_work_id = c.work_id
             )
             SELECT work_id FROM component",
        params![release_id],
        |row| row.get(0),
    )?;

    // Composers link only through work_artists, so add them as candidates.
    for work_id in &candidate_work_ids {
        candidate_artist_ids.extend(sql.query(
            "SELECT artist_id FROM work_artists WHERE work_id = ?1",
            params![work_id],
            |row| row.get::<_, String>(0),
        )?);
    }

    // Works that survive once this release's track_works are gone — reachable from
    // a track on another release, directly or through the work_parts hierarchy.
    let live_work_ids: std::collections::HashSet<String> = sql
        .query(
            "WITH RECURSIVE live(work_id) AS (
                 SELECT DISTINCT work_id FROM track_works
                   WHERE track_id NOT IN (SELECT id FROM tracks WHERE release_id = ?1)
                 UNION
                 SELECT wp.parent_work_id FROM work_parts wp
                   JOIN live l ON wp.child_work_id = l.work_id
                 UNION
                 SELECT wp.child_work_id FROM work_parts wp
                   JOIN live l ON wp.parent_work_id = l.work_id
             )
             SELECT work_id FROM live",
            params![release_id],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .collect();
    let orphaned_work_ids: Vec<String> = candidate_work_ids
        .into_iter()
        .filter(|work_id| !live_work_ids.contains(work_id))
        .collect();

    let mut image_blobs: Vec<(&'static str, String, Option<String>)> = Vec::new();
    // The cover is 1:1 with the release, so it always orphans.
    if let Some((blob_id, cloud_path)) = sql
        .query_row(
            "SELECT blob_id, cloud_path FROM covers WHERE id = ?",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    {
        image_blobs.push((crate::sync::COVERS_NAMESPACE, blob_id, cloud_path));
    }

    // An artist orphans when nothing outside the deleted subtree names it. The
    // work_artists clause excludes the orphaned works (their rows cascade away);
    // an empty orphaned-work set drops the exclusion so every work_artists row
    // counts as surviving.
    let work_exclusion = if orphaned_work_ids.is_empty() {
        String::new()
    } else {
        let placeholders: Vec<String> = (0..orphaned_work_ids.len())
            .map(|i| format!("?{}", i + 5))
            .collect();
        format!("AND work_id NOT IN ({})", placeholders.join(", "))
    };
    let orphan_check = format!(
        "SELECT NOT (
             EXISTS(SELECT 1 FROM albums WHERE artist_id = ?1 AND NOT (id = ?2 AND ?3))
             OR EXISTS(SELECT 1 FROM album_artists WHERE artist_id = ?1 AND NOT (album_id = ?2 AND ?3))
             OR EXISTS(SELECT 1 FROM track_artists WHERE artist_id = ?1
                        AND track_id NOT IN (SELECT id FROM tracks WHERE release_id = ?4))
             OR EXISTS(SELECT 1 FROM work_artists WHERE artist_id = ?1 {work_exclusion})
             OR EXISTS(SELECT 1 FROM release_artist_roles WHERE artist_id = ?1 AND release_id != ?4)
             OR EXISTS(SELECT 1 FROM track_artist_roles WHERE artist_id = ?1
                        AND track_id NOT IN (SELECT id FROM tracks WHERE release_id = ?4))
         )"
    );

    let mut orphaned_artist_ids: Vec<String> = Vec::new();
    for artist_id in &candidate_artist_ids {
        let mut binds: Vec<&dyn coven::rusqlite::ToSql> =
            vec![artist_id, &album_id, &delete_album, &release_id];
        for work_id in &orphaned_work_ids {
            binds.push(work_id);
        }
        let orphaned: bool = sql.query_row(&orphan_check, binds.as_slice(), |row| row.get(0))?;
        if orphaned {
            orphaned_artist_ids.push(artist_id.clone());
            if let Some((blob_id, cloud_path)) = sql
                .query_row(
                    "SELECT blob_id, cloud_path FROM artist_images WHERE id = ?",
                    params![artist_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .optional()?
            {
                image_blobs.push((crate::sync::ARTIST_IMAGES_NAMESPACE, blob_id, cloud_path));
            }
        }
    }

    let file_ids: Vec<String> = sql.query(
        "SELECT id FROM release_files WHERE release_id = ?",
        params![release_id],
        |row| row.get(0),
    )?;

    Ok(FailImportDeletion {
        release_id: release_id.to_string(),
        album_id,
        delete_album,
        orphaned_work_ids,
        orphaned_artist_ids,
        image_blobs,
        file_ids,
    })
}
