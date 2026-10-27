#[cfg(desktop)]
use super::import_state::{
    end_import_attempt_of_commit, record_import_origin_on, require_import_commit_guard,
};
use super::*;
mod storage;

/// The row lists an import commit writes around its release — the artist, work,
/// role, audio-format, and record rows that hang off it.
///
/// One group rather than thirteen parameters: each is a plain list the caller
/// either has or does not, and `Default` is every one of them empty, so a
/// caller names only the lists it actually fills.
///
/// Every artist link here — `album_artists`, `track_artists`, the role rows and `work_artists` — names its artist by the
/// id of one of `artists.credits`. The commit resolves those credits to
/// library artists inside its own transaction and writes each link to the
/// artist its credit resolved to.
#[cfg(desktop)]
#[derive(Default)]
pub(crate) struct ImportRows<'a> {
    pub mediums: &'a [DbReleaseMedium],
    pub track_artists: &'a [DbTrackArtist],
    pub album_artists: &'a [DbAlbumArtist],
    pub works: &'a [DbWork],
    pub work_artists: &'a [DbWorkArtist],
    pub work_parts: &'a [DbWorkPart],
    pub track_works: &'a [DbTrackWork],
    pub release_artist_roles: &'a [DbReleaseArtistRole],
    pub track_artist_roles: &'a [DbTrackArtistRole],
    pub artists: super::artist_resolution::ArtistCredits<'a>,
    pub audio_formats: &'a [DbAudioFormat],
    pub audio_segments: &'a [DbAudioSegment],
    pub scrub_points: &'a [DbScrubPoints],
    pub records: &'a [crate::import::ReleaseRecord],
}

/// Pictures for the new artists an import expects to create. Their bytes are
/// staged before the transaction's SQL runs, so they are prepared against a
/// read of the library just before it; the transaction resolves the credits
/// again and refuses to commit if the new artists it would create are not
/// exactly `expected_new_artists` — a picture is never left without its
/// artist, and no new artist misses the picture prepared for it.
#[cfg(desktop)]
#[derive(Default, Clone, Copy)]
pub(crate) struct NewArtistImages<'a> {
    pub expected_new_artists: &'a [String],
    pub images: &'a [(&'a DbLibraryImage, &'a [u8])],
}

/// A release's metadata as an edit leaves it: album fields, release pressing
/// fields, the track rows named by `track_updates` (existing track ID → edited
/// row), and the album's and tracks' artist links.
///
/// Every link names its artist by the id of one of `credits`; the credits are resolved to library artists inside the write
/// and each link is written to the artist its credit resolved to.
pub(crate) struct ReleaseMetadataRows {
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
pub(crate) enum AlbumFields {
    Written,
    #[cfg_attr(
        not(desktop),
        expect(
            dead_code,
            reason = "only a desktop edit session moves a release into an album the person picked"
        )
    )]
    Kept,
}

/// Write `release`'s pressing over its row.
pub(super) fn update_release_pressing_on(
    tx: &SqlContext<'_, '_>,
    release: &DbRelease,
) -> Result<(), DbError> {
    let facts = super::pressing_columns::FactColumns::of(&release.pressing.facts);
    tx.execute(
        r#"UPDATE releases SET year = ?, labels = ?, barcodes = ?,
            country = ?, region = ?, media = ?, status = ?, packaging = ?,
            discogs_details = ? WHERE id = ?"#,
        params![
            release.pressing.year,
            super::pressing_columns::labels_column(&release.pressing.labels),
            super::pressing_columns::barcodes_column(&release.pressing.barcodes),
            facts.country,
            facts.region,
            facts.media,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            release.id,
        ],
    )?;
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
    tx: &SqlContext<'_, '_>,
    metadata: &ReleaseMetadataRows,
    album_fields: AlbumFields,
    now: &str,
) -> Result<(), ArtistWriteError> {
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
    let album_artists = relink_album_artists(album_artists, &artist_ids)?;
    let track_artists = relink_track_artists(track_artists, &artist_ids)?;

    if album_fields == AlbumFields::Written {
        tx.execute(
            r#"UPDATE albums SET title = ?, year = ?, is_compilation = ? WHERE id = ?"#,
            params![album.title, album.year, album.is_compilation, album.id,],
        )?;
        replace_album_artists(tx, &album.id, &album_artists, now)?;
    }

    update_release_pressing_on(tx, release)?;

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

impl Database {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release(&self, release: &DbRelease) -> Result<(), DbError> {
        let release = release.clone();
        self.call_sql(move |sql| insert_release_row(&sql, &release))
            .await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_composition_fixture_rows(
        &self,
        works: &[DbWork],
        track_works: &[DbTrackWork],
    ) -> Result<(), DbError> {
        let (works, track_works) = (works.to_vec(), track_works.to_vec());
        self.call_sql(move |sql| {
            let conn = &sql;
            for work in &works {
                insert_work_row(conn, work)?;
            }
            for track_work in &track_works {
                insert_track_work_row(conn, track_work)?;
            }
            Ok(())
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
    ) -> Result<(), DbError> {
        let (album, album_artists, release, mediums, tracks, track_artists) = (
            album.clone(),
            album_artists.to_vec(),
            release.clone(),
            mediums.to_vec(),
            tracks.to_vec(),
            track_artists.to_vec(),
        );
        self.call_sql(move |sql| {
            let tx = &sql;

            insert_album_row(tx, &album)?;
            for credit in &album_artists {
                insert_album_artist_row(tx, credit)?;
            }
            insert_release_row(tx, &release)?;
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
    ) -> Result<(), DbError> {
        let (release, mediums, tracks, track_artists) = (
            release.clone(),
            mediums.to_vec(),
            tracks.to_vec(),
            track_artists.to_vec(),
        );
        self.call_sql(move |sql| {
            let tx = &sql;

            insert_release_row(tx, &release)?;
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
    /// [`write_release_metadata_on`]).
    pub(crate) async fn update_release_metadata_user_edit(
        &self,
        metadata: ReleaseMetadataRows,
    ) -> Result<(), ArtistWriteError> {
        let now = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now()).to_rfc3339();
        self.write_resolving_artists(move |sql| {
            write_release_metadata_on(&sql, &metadata, AlbumFields::Written, &now)
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
        .process(|rows| Ok(rows.map(ReleaseDetailRows::process)))
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
        self.read(move |sql| find_release_detail_context_on(&sql, &release_id))
            .process(|rows| Ok(rows.map(super::release_projection::ReleaseContextRows::process)))
            .await
    }

    /// Follow the release `initial` names, or none. The detail view moves
    /// to another release through the request handle, not another query.
    pub(crate) fn subscribe_release_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ReleaseDetailProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |release_id, sql| {
                let Some(release_id) = release_id else {
                    return Ok((None, HashMap::new()));
                };
                let context =
                    find_release_detail_context_on(&sql, release_id).map_err(CovenError::from)?;
                let cover_versions = super::files::image_versions_on(
                    &sql,
                    LibraryImageType::Cover,
                    std::slice::from_ref(release_id),
                )
                .map_err(CovenError::from)?;
                Ok((context, cover_versions))
            })
            .process(|(context, cover_versions)| {
                Ok(ReleaseDetailProjection {
                    context: context.map(super::release_projection::ReleaseContextRows::process),
                    cover_versions,
                })
            })
    }

    /// Ordered by `created_at`.
    pub async fn get_releases_for_album(&self, album_id: &str) -> Result<Vec<DbRelease>, DbError> {
        let album_id = album_id.to_string();
        self.read(move |sql| get_releases_for_album_on(&sql, &album_id))
            .await
    }
    /// Seed a release-file row for tests that do not exercise its file bytes.
    /// Production creates the row and its Coven-owned hash together in
    /// `finalize_import_atomic`.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_file(&self, file: &DbFile) -> Result<(), DbError> {
        let file = file.clone();
        self.call_sql(move |sql| insert_file_row_for_test(&sql, &file))
            .await
    }

    /// A local release's original is device-local until explicitly attached.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_external_file_for_test(
        &self,
        file: &DbFile,
        path: &std::path::Path,
    ) -> Result<(), DbError> {
        let metadata = tokio::fs::metadata(path).await?;
        if metadata.len()
            != u64::try_from(file.file_size).map_err(|error| DbError::Message(error.to_string()))?
        {
            return Err(DbError::LocalFileChanged(path.into()));
        }
        let path = path
            .to_str()
            .ok_or_else(|| DbError::Message("local path is not UTF-8".into()))?
            .to_string();
        let modified = DateTime::<Utc>::from(metadata.modified()?).to_rfc3339();
        let file = file.clone();
        self.call_sql(move |sql| {
            insert_file_row_for_test(&sql, &file)?;
            sql.execute(
                "INSERT INTO local_release_files(file_id,path,modified_at) VALUES(?1,?2,?3)",
                params![file.id, path, modified],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_files_for_release(&self, release_id: &str) -> Result<Vec<DbFile>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| get_files_for_release_on(&sql, &release_id))
            .process(|files| Ok(process_files(files)))
            .await
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
        self.call_sql(move |sql| {
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
        self.call_sql(move |sql| insert_scrub_points_row(&sql, &points))
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
    #[cfg(desktop)]
    pub(crate) async fn finalize_import_atomic(
        &self,
        guard: ImportCommitGuard,
        // `None` when the album is an existing one, already in the DB.
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        rows: ImportRows<'_>,
        files: Vec<crate::import::service::PreparedImportFile>,
        library_image: Option<(&DbLibraryImage, &[u8])>,
        artist_images: NewArtistImages<'_>,
    ) -> Result<(), ArtistWriteError> {
        let album = album.cloned();
        let release = release.clone();
        let tracks: Vec<DbTrack> = tracks_to_files
            .iter()
            .map(|tf| tf.db_track.clone())
            .collect();
        let mediums = rows.mediums.to_vec();
        let track_artists = rows.track_artists.to_vec();
        let album_artists = rows.album_artists.to_vec();
        let works = rows.works.to_vec();
        let work_artists = rows.work_artists.to_vec();
        let work_parts = rows.work_parts.to_vec();
        let track_works = rows.track_works.to_vec();
        let release_artist_roles = rows.release_artist_roles.to_vec();
        let track_artist_roles = rows.track_artist_roles.to_vec();
        let artist_credits = rows.artists.credits.to_vec();
        let picked_artists = rows.artists.picked.to_vec();
        let expected_new_artists: BTreeSet<String> =
            artist_images.expected_new_artists.iter().cloned().collect();
        let audio_formats = rows.audio_formats.to_vec();
        let audio_segments = rows.audio_segments.to_vec();
        let scrub_points = rows.scrub_points.to_vec();
        let library_image = library_image.map(|(image, bytes)| (image.clone(), bytes.to_vec()));
        let artist_images: Vec<(DbLibraryImage, Vec<u8>)> = artist_images
            .images
            .iter()
            .map(|(image, bytes)| ((*image).clone(), (*bytes).to_vec()))
            .collect();
        // Each image's bytes attach to its row, which coven finds by the row's id.
        let image_files: Vec<(String, String, Vec<u8>)> = library_image
            .iter()
            .chain(artist_images.iter())
            .map(|(image, bytes)| {
                (
                    image.image_type.namespace().to_string(),
                    image.id.clone(),
                    bytes.clone(),
                )
            })
            .collect();
        let records = rows.records.to_vec();
        let now_dt = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now());
        let now = now_dt.to_rfc3339();
        self.write_with_files(
            move |w| {
                for (namespace, id, bytes) in image_files {
                    w.put_file(namespace, id, bytes);
                }
                Ok(())
            },
            move |sql| {
                let tx = &sql;
                require_import_commit_guard(tx, &guard)?;
                end_import_attempt_of_commit(tx, &guard)?;

                // Every credit is decided here, against the library as it
                // stands inside this transaction, so an artist another
                // import committed a moment ago is the one this import
                // links to. New artists go in and linked ones are filled in
                // before the links below point at them.
                let credits = ArtistCredits {
                    credits: &artist_credits,
                    picked: &picked_artists,
                };
                let resolved = credits
                    .resolve_for_import_on(tx, guard.content_hash())
                    .map_err(artist_write_failure)?;
                let inserted: BTreeSet<String> = resolved
                    .inserts
                    .iter()
                    .map(|artist| artist.id.clone())
                    .collect();
                if inserted != expected_new_artists {
                    return Err(artist_write_failure(ArtistWriteError::LibraryChanged));
                }
                resolved.write_on(tx)?;
                let artist_ids = resolved.id_map(&artist_credits);
                let album_artists = relink_album_artists(&album_artists, &artist_ids)?;
                let track_artists = relink_track_artists(&track_artists, &artist_ids)?;
                let release_artist_roles = relink(
                    &release_artist_roles,
                    &artist_ids,
                    "release artist role",
                    |role| &mut role.artist_id,
                )?;
                let track_artist_roles = relink(
                    &track_artist_roles,
                    &artist_ids,
                    "track artist role",
                    |role| &mut role.artist_id,
                )?;
                let work_artists = relink(&work_artists, &artist_ids, "work artist", |link| {
                    &mut link.artist_id
                })?;

                // A new album for a release group carries the group's id, so
                // its row can be here already, emptied by an earlier delete
                // or move: the import writes it whole either way.
                if let Some(album) = &album {
                    upsert_album_row(tx, album)?;
                    replace_album_artists(tx, &album.id, &album_artists, &now)?;
                }

                insert_release_row(tx, &release)?;
                record_import_origin_on(tx, &guard, &release.id).map_err(artist_write_failure)?;

                // One row per catalog that describes the release, empty when
                // none does. The records are uniquely keyed on
                // `(release_id, catalog)`, so a release never carries two
                // rows for one catalog.
                write_release_records(tx, &release.id, &records, &now)?;

                // Works are globally identified; they go in before their links.
                for work in &works {
                    insert_work_row(tx, work)?;
                }

                for work_artist in &work_artists {
                    insert_work_artist_row(tx, work_artist)?;
                }

                for work_part in &work_parts {
                    insert_work_part_row(tx, work_part)?;
                }

                for role in &release_artist_roles {
                    insert_release_artist_role_row(tx, role)?;
                }

                for medium in &mediums {
                    insert_release_medium_row(tx, medium)?;
                }

                // The tracks came out of `tracks_to_files`; their `duration_ms`
                // was set from the audio each is bound to: its CUE slice or its
                // whole file.
                for track in &tracks {
                    insert_track_row(tx, track)?;
                }

                for ta in &track_artists {
                    insert_track_artist_row(tx, ta)?;
                }

                for track_work in &track_works {
                    insert_track_work_row(tx, track_work)?;
                }

                for role in &track_artist_roles {
                    insert_track_artist_role_row(tx, role)?;
                }

                for prepared in files {
                    match prepared.source {
                        crate::import::service::PreparedImportSource::Local(original) => {
                            insert_local_file_row(tx, &prepared.row, &original)?;
                        }
                        crate::import::service::PreparedImportSource::Shared {
                            original,
                            attachment,
                        } => {
                            insert_external_file_row(tx, &prepared.row, &original, attachment)?;
                        }
                    }
                }

                for af in &audio_formats {
                    insert_audio_format_row(tx, af)?;
                }
                for segment in &audio_segments {
                    insert_audio_segment_row(tx, segment)?;
                }
                for points in &scrub_points {
                    insert_scrub_points_row(tx, points)?;
                }

                if let Some((image, _)) = &library_image {
                    upsert_library_image_row(tx, image)?;
                }
                for (image, _) in &artist_images {
                    upsert_library_image_row(tx, image)?;
                }

                Ok(())
            },
        )
        .await
        .map(|_| ())
        .map_err(Self::artist_write_error)
    }

    /// Delete a release in one write. Its album's next release in album order
    /// is then the primary; nothing else is written for that.
    /// Shared artists and works remain available to other releases and devices.
    /// Deleting abandons the attachment of each of its files this device is
    /// still uploading; the person asked for that, so its record is dismissed.
    pub async fn delete_release_with_cleanup(
        &self,
        plan: ReleaseDeletion,
    ) -> Result<bool, DbError> {
        let abandoned = self.own_abandonments(&plan.file_rows()).await?;
        let vacated = self
            .call_sql(move |sql| {
                plan.apply_on(&sql)?;
                sql.execute(
                    "DELETE FROM releases WHERE id = ?",
                    params![plan.release_id()],
                )?;
                vacate_album_on(&sql, plan.album_id())
            })
            .await?;
        self.dismiss_own_abandonments(&abandoned).await;
        Ok(vacated)
    }

    /// Read what deleting `release_id` owes coven; see [`ReleaseDeletion`].
    pub async fn plan_release_deletion(
        &self,
        release_id: &str,
    ) -> Result<ReleaseDeletion, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| ReleaseDeletion::plan_on(&sql, &release_id))
            .await
    }

    /// Find release by ID. Caller-provided ID — may not exist.
    pub async fn find_release_by_id(&self, release_id: &str) -> Result<Option<DbRelease>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| find_release_by_id_on(&sql, &release_id))
            .await
    }

    /// Test-only: write a path fragment onto an existing row directly, the way a
    /// changeset from another device does. coven applies a pulled row straight into
    /// SQLite, so the validation on the row-write never sees it — which is why
    /// export and audio detachment validate the fragment again before joining it onto a
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

    /// Test-only: write a release's media column as text, the way a changeset
    /// another device wrote lands, so a test can make the release's read fail.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn set_release_media_text_for_test(
        &self,
        release_id: &str,
        media: &str,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let media = media.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE releases SET media = ?1 WHERE id = ?2",
                params![media, release_id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
}
