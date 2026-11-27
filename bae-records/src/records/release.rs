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

/// Whether a release's write writes what its pressing is — area, media,
/// status, packaging and Discogs details — or keeps what the library holds,
/// as an edit of typed values alone does.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PressingFactsWrite {
    Written,
    Kept,
}

/// Write `release`'s pressing over its row, its labels naming the library
/// labels `pressing` decides.
#[cfg(desktop)]
pub(super) fn update_release_pressing_on(
    tx: &CovenWrite<'_>,
    release: &DbRelease,
    pressing: &super::PressingWrite<'_>,
) -> Result<(), IdentityWriteError> {
    update_release_typed_pressing_on(tx, release, pressing)?;
    let facts = &release.pressing.facts;
    update_release_fact_columns_on(tx, &release.id, facts)?;
    super::write::replace_release_media(tx, &release.id, &facts.media, pressing.media_ids)?;
    super::write::replace_release_discogs_details(
        tx,
        &release.id,
        &facts.discogs_details,
        pressing.discogs_detail_ids,
    )?;
    Ok(())
}

/// Write what is typed of `release`'s pressing: its year, labels and
/// barcodes.
#[cfg(desktop)]
fn update_release_typed_pressing_on(
    tx: &CovenWrite<'_>,
    release: &DbRelease,
    pressing: &super::PressingWrite<'_>,
) -> Result<(), IdentityWriteError> {
    tx.execute(
        "UPDATE releases SET year = ? WHERE id = ?",
        params![release.pressing.typed.year, release.id],
    )?;
    super::label_resolution::write_release_labels_on(
        tx,
        &release.id,
        &release.pressing.typed.labels,
        pressing,
    )?;
    super::write::replace_release_barcodes(
        tx,
        &release.id,
        &release.pressing.typed.barcodes,
        pressing.barcode_ids,
    )?;
    Ok(())
}

/// Write the facts of `facts` the release `release_id`'s own row keeps: its
/// area, status and packaging.
fn update_release_fact_columns_on(
    tx: &CovenWrite<'_>,
    release_id: &str,
    facts: &bae_types::pressing::PressingFacts,
) -> Result<(), DbError> {
    let facts = super::pressing_columns::FactColumns::of(facts);
    tx.execute(
        "UPDATE releases SET country = ?, region = ?, status = ?, packaging = ? WHERE id = ?",
        params![facts.country, facts.region, facts.status, facts.packaging, release_id],
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
#[cfg(desktop)]
pub(super) fn write_release_metadata_on(
    tx: &CovenWrite<'_>,
    metadata: &ReleaseMetadataRows,
    album_fields: AlbumFields,
    pressing_facts: PressingFactsWrite,
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

    match pressing_facts {
        PressingFactsWrite::Written => update_release_pressing_on(tx, release, pressing)?,
        PressingFactsWrite::Kept => update_release_typed_pressing_on(tx, release, pressing)?,
    }

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

    /// Make `change` to what the release `release_id`'s pressing is, reading
    /// and writing it in one transaction, so the change lands on the facts
    /// the library holds then. Only what the change makes different is
    /// written. `false` when the library holds no such release.
    pub async fn change_release_pressing_facts(
        &self,
        release_id: &str,
        change: impl FnOnce(&mut bae_types::pressing::PressingFacts) + Send + 'static,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        let ids = self.ids.clone();
        self.store
            .write(move |sql| {
                let Some(release) = find_release_by_id_on(&sql, &release_id)? else {
                    return Ok::<_, DbError>(false);
                };
                let held = release.pressing.facts;
                let mut facts = held.clone();
                change(&mut facts);
                if (facts.area, facts.status, facts.packaging)
                    != (held.area, held.status, held.packaging)
                {
                    update_release_fact_columns_on(&sql, &release_id, &facts)?;
                }
                let fresh =
                    |count: usize| (0..count).map(|_| ids.new_id().to_string()).collect::<Vec<_>>();
                if facts.media != held.media {
                    super::write::replace_release_media(
                        &sql,
                        &release_id,
                        &facts.media,
                        &fresh(facts.media.len()),
                    )?;
                }
                if facts.discogs_details != held.discogs_details {
                    super::write::replace_release_discogs_details(
                        &sql,
                        &release_id,
                        &facts.discogs_details,
                        &fresh(facts.discogs_details.len()),
                    )?;
                }
                Ok(true)
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
    /// The release file `file_id`, of whichever release holds it. Caller-provided
    /// ID — may not exist.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, DbError> {
        let file_id = file_id.to_string();
        self.store.read(move |sql| super::release_with_files::release_file_on(&sql, &file_id)).await
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

// The release `release` as its editor's draft shows it beside the form:
// whether it has tracks, its records, whether the draft shows any record (the
// form's `records`, a JSON array, once the form replaced the release's, and
// else the release's own) and its cover's version; no row when the library
// holds no such release.
sqlive_compose::read!(
    writer EditedReleaseHead(release: String, relinked: bool, records: String) {
        has_tracks: bool,
        shows_records: bool
    },
    r#"
    SELECT EXISTS (SELECT 1 FROM tracks AS held WHERE held.release_id = rel.id) AS has_tracks,
           CASE WHEN :relinked THEN json_array_length(:records) > 0
                ELSE EXISTS (SELECT 1 FROM release_records AS linked
                             WHERE linked.release_id = rel.id) END AS shows_records,
           described.*, cover.*
    FROM releases AS rel
    JOIN @crate::records::fragments::ReleaseRecords AS described
      ON described.records_release_id = rel.id
    LEFT JOIN @crate::records::files::CoverVersion AS cover ON cover.id = rel.id
    WHERE rel.id = :release
"#
);

/// A release as its editor's draft shows it beside the form.
#[derive(Debug, Clone, PartialEq)]
pub struct EditedRelease {
    /// The version of the release's cover; `None` when it has none.
    pub cover_version: Option<String>,
    pub has_tracks: bool,
    /// Whether the draft shows any record: the form's, once it replaced the
    /// release's records, and else the release's own.
    pub shows_records: bool,
    /// The records of the release as the library holds them.
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
}

/// The release `release_id` as its editor's draft shows it; `None` when the
/// library holds no such release. `form_records` is the JSON array of the
/// records the form replaced the release's with, where it replaced them.
pub fn edited_release_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
    form_records: Option<&str>,
) -> Result<Option<EditedRelease>, DbError> {
    let params = EditedReleaseHeadParams {
        release: release_id.to_string(),
        relinked: form_records.is_some(),
        records: form_records.unwrap_or("[]").to_string(),
    };
    bae_store::statement_optional(sql, &params)?
        .map(|row| {
            Ok(EditedRelease {
                cover_version: row.cover.and_then(super::files::CoverVersion::into_version),
                has_tracks: row.has_tracks,
                shows_records: row.shows_records,
                records: super::read::records_of_json(row.described.records_json())?,
            })
        })
        .transpose()
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
