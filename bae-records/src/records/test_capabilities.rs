use super::*;
use bae_store::statement_rows;

// Whether the cover of the release `:release` waits in this device's upload
// queue.
coven::read!(
    writer CoverQueued(release: String) { queued: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM covers AS cover
                   JOIN coven_uploads AS upload ON upload.file = cover.browse_file
                   WHERE cover.id = :release) AS queued
"#
);

// The labels of the release `:release`, each with the label it shows as.
#[cfg(test)]
coven::read!(
    writer ShownLabels(release: String),
    r#"
    SELECT @crate::records::pressing_columns::ReleaseLabels(parent: rel) AS labels
    FROM releases AS rel
    WHERE rel.id = :release
"#
);

// How many of the files waiting in this device's upload queue release files
// reference.
coven::read!(
    writer QueuedReleaseFileCount { queued: i64 },
    r#"
    SELECT COUNT(*) AS queued FROM coven_uploads AS upload
    JOIN release_files AS file ON file.file = upload.file
"#
);

// How many of the files waiting in this device's upload queue covers
// reference.
coven::read!(
    writer QueuedCoverCount { queued: i64 },
    r#"
    SELECT COUNT(*) AS queued FROM coven_uploads AS upload
    JOIN covers AS cover ON cover.browse_file = upload.file
"#
);

// The release `:release`'s own files and its cover that wait in this
// device's upload queue, each with its table.
coven::read!(
    writer QueuedUploadsOfRelease(release: String) { queued_table: String, queued_id: String },
    r#"
    SELECT 'release_files' AS queued_table, file.id AS queued_id
    FROM release_files AS file
    JOIN coven_uploads AS upload ON upload.file = file.file
    WHERE file.release_id = :release
    UNION ALL
    SELECT 'covers' AS queued_table, cover.id AS queued_id
    FROM covers AS cover
    JOIN coven_uploads AS upload ON upload.file = cover.browse_file
    WHERE cover.id = :release
"#
);

// The works the MusicBrainz work `:work` identifies.
coven::read!(
    writer WorksIdentifiedAs(work: String),
    "SELECT work.id FROM works AS work WHERE work.musicbrainz_work_id = :work"
);

// The releases whose tracks perform a work the MusicBrainz work `:work`
// identifies, in id order.
coven::read!(
    writer ReleasesPerformingWork(work: String),
    r#"
    SELECT DISTINCT track.release_id
    FROM track_works AS link
    JOIN tracks AS track ON track.id = link.track_id
    JOIN works AS work ON work.id = link.work_id
    WHERE work.musicbrainz_work_id = :work
    ORDER BY track.release_id
"#
);

// The tracks of the release `:release` in play order.
coven::read!(
    writer TracksInOrder(release: String),
    r#"
    SELECT track.id, track.title FROM tracks AS track
    WHERE track.release_id = :release ORDER BY track.position
"#
);

// The artists credited on the track `:track` in a role, in order: each its
// MusicBrainz id, else its name.
coven::read!(
    writer RoleArtistsOfTrack(track: String) { source: String },
    r#"
    SELECT COALESCE(artist.musicbrainz_artist_id, artist.name) AS source
    FROM track_artist_roles AS role
    JOIN artists AS artist ON artist.id = role.artist_id
    WHERE role.track_id = :track ORDER BY role.position
"#
);

// The MusicBrainz ids of the works the track `:track` performs, in order.
coven::read!(
    writer WorkSourcesOfTrack(track: String) { musicbrainz_work_id: String },
    r#"
    SELECT work.musicbrainz_work_id FROM track_works AS link
    JOIN works AS work ON work.id = link.work_id
    WHERE link.track_id = :track ORDER BY link.position
"#
);

// Each track of the release `:release` in play order, with the original
// name of the file its main audio segment reads.
coven::read!(
    writer CommittedTrackFiles(release: String),
    r#"
    SELECT track.title, file.original_filename
    FROM tracks AS track
    JOIN audio_formats AS format ON format.track_id = track.id
    JOIN audio_format_segments AS segment
      ON segment.audio_format_id = format.id AND segment.role = 'main'
    JOIN release_files AS file ON file.id = segment.file_id
    WHERE track.release_id = :release
    ORDER BY track.position
"#
);

// The track `:track`'s own credits in list order, each with the other
// printed name it names, if any.
coven::read!(
    writer TrackCreditRows(track: String) { position: u32 },
    r#"
    SELECT credit.artist_id, credit.position, credit.join_phrase,
           credit_name.name_key, credit_name.name
    FROM track_artists AS credit
    LEFT JOIN artist_credits AS credit_name
           ON credit_name.artist_id = credit.artist_id
          AND credit_name.name_key = credit.credit_key
    WHERE credit.track_id = :track
    ORDER BY credit.position
"#
);

// The title of the album `:album`.
#[cfg(test)]
coven::read!(
    writer TitleOfAlbum(album: String),
    "SELECT album.title FROM albums AS album WHERE album.id = :album"
);

impl Records {
    /// Whether the cover of the release `release_id` waits in this device's
    /// upload queue, live.
    pub fn subscribe_cover_queued_for_test(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<bool, DbError> {
        let statement = CoverQueuedParams { release: release_id.to_string() };
        self.store.subscribe_row(statement).process(|row| Ok(row.queued))
    }

    /// How many of the files waiting in this device's upload queue release
    /// files reference.
    pub async fn queued_release_file_count_for_test(&self) -> Result<i64, DbError> {
        Ok(self.store.read_row(QueuedReleaseFileCountParams {}).await?.queued)
    }

    /// How many of the files waiting in this device's upload queue covers
    /// reference.
    pub async fn queued_cover_count_for_test(&self) -> Result<i64, DbError> {
        Ok(self.store.read_row(QueuedCoverCountParams {}).await?.queued)
    }

    pub async fn queued_upload_count_for_release_for_test(
        &self,
        release_id: &str,
    ) -> Result<usize, DbError> {
        Ok(self.queued_upload_rows_for_release_for_test(release_id).await?.len())
    }

    pub async fn queued_upload_rows_for_release_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        let statement = QueuedUploadsOfReleaseParams { release: release_id.to_string() };
        let rows = self.store.read_rows(statement).await?;
        Ok(rows.into_iter().map(|row| (row.queued_table, row.queued_id)).collect())
    }

    pub async fn work_links_for_test(
        &self,
        musicbrainz_work_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), DbError> {
        let work = musicbrainz_work_id.to_string();
        self.store
            .read(move |sql| {
                let works = statement_rows(&sql, &WorksIdentifiedAsParams { work: work.clone() })?;
                let releases = statement_rows(&sql, &ReleasesPerformingWorkParams { work })?;
                Ok((
                    works.into_iter().map(|row| row.id).collect(),
                    releases.into_iter().map(|row| row.release_id).collect(),
                ))
            })
            .await
    }

    /// Each track of the release in play order: its title, the MusicBrainz
    /// ids of the artists credited on it in a role, and of the works it
    /// performs, each in order.
    pub async fn track_sources_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, Vec<String>, Vec<String>)>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                let tracks = statement_rows(&sql, &TracksInOrderParams { release: release_id })?;
                let mut out = Vec::new();
                for TracksInOrderRow { id, title } in tracks {
                    let roles =
                        statement_rows(&sql, &RoleArtistsOfTrackParams { track: id.clone() })?;
                    let works = statement_rows(&sql, &WorkSourcesOfTrackParams { track: id })?;
                    out.push((
                        title,
                        roles.into_iter().map(|row| row.source).collect(),
                        works.into_iter().map(|row| row.musicbrainz_work_id).collect(),
                    ));
                }
                Ok(out)
            })
            .await
    }

    pub async fn committed_track_files_for_test(
        &self,
        release_id: &str,
    ) -> Result<Vec<(String, String)>, DbError> {
        let statement = CommittedTrackFilesParams { release: release_id.to_string() };
        let rows = self.store.read_rows(statement).await?;
        Ok(rows.into_iter().map(|row| (row.title, row.original_filename)).collect())
    }
}

impl Records {
    /// Write `rows` in one write through the records' own row inserts, each
    /// kind in the order [`LibraryRows`] lists them.
    ///
    /// [`LibraryRows`]: crate::test_fixtures::library::LibraryRows
    pub async fn insert_library_rows_for_test(
        &self,
        rows: crate::test_fixtures::library::LibraryRows,
    ) -> Result<(), IdentityWriteError> {
        let pressing = rows.pressing_parts(&*self.ids, self.store.now_utc());
        self.store.write(move |sql| rows.write_on(&sql, &pressing)).await
    }

    /// Save `release_id`'s metadata as a person's edit of it does: the
    /// album, the release, its tracks and their credits as they stand, with
    /// what `edit` changes of them. The save keeps what the pressing is, so
    /// an `edit` that changes it panics:
    /// [`Records::change_release_pressing_facts`] changes that.
    #[cfg(desktop)]
    pub async fn save_release_metadata_for_test(
        &self,
        release_id: &str,
        edit: impl FnOnce(&mut ReleaseMetadataRows),
    ) -> Result<(), crate::ReleaseEditSaveError> {
        let read = self.find_release_to_edit(release_id).await?.expect("the release");
        let mut rows = self.release_metadata_for_test(release_id).await?;
        let facts = rows.release.pressing.facts.clone();
        edit(&mut rows);
        assert_eq!(
            rows.release.pressing.facts, facts,
            "a metadata save keeps what the pressing is: change_release_pressing_facts changes it"
        );
        self.update_release_metadata_user_edit(rows, read).await
    }

    /// `release_id`'s metadata as an edit of it starts from: the album, the
    /// release, its tracks and their credits as they stand, every credited
    /// artist picked.
    pub(crate) async fn release_metadata_for_test(
        &self,
        release_id: &str,
    ) -> Result<ReleaseMetadataRows, DbError> {
        let release = self.find_release_by_id(release_id).await?.expect("the release");
        let album = self.find_album_by_id(&release.album_id).await?.expect("its album");
        let tracks = self.get_tracks_for_release(release_id).await?;
        let album_id = album.id.clone();
        let track_ids: Vec<String> = tracks.iter().map(|track| track.id.clone()).collect();
        let (album_credits, track_credits) = self
            .store
            .read(move |sql| {
                let mut track_credits = Vec::new();
                for track_id in track_ids {
                    let statement = TrackCreditRowsParams { track: track_id.clone() };
                    let credits: Vec<CreditForTest> = statement_rows(&sql, &statement)?
                        .into_iter()
                        .map(|row| {
                            credit_for_test(
                                row.artist_id,
                                row.position,
                                row.name_key,
                                row.name,
                                row.join_phrase,
                            )
                        })
                        .collect::<Result<_, _>>()?;
                    track_credits.push((track_id, credits));
                }
                let statement = super::release_records::AlbumCreditRowsParams { album: album_id };
                let album_credits: Vec<CreditForTest> = statement_rows(&sql, &statement)?
                    .into_iter()
                    .map(|row| {
                        credit_for_test(
                            row.artist_id,
                            row.position,
                            row.name_key,
                            row.name,
                            row.join_phrase,
                        )
                    })
                    .collect::<Result<_, _>>()?;
                Ok((album_credits, track_credits))
            })
            .await?;
        let mut credits = Vec::new();
        let credited = album_credits.iter().chain(track_credits.iter().flat_map(|(_, list)| list));
        for (artist_id, ..) in credited {
            credits.push(self.find_artist_by_id(artist_id).await?.expect("a credited artist"));
        }
        let now = self.store.now_utc();
        let album_artists = album_credits
            .into_iter()
            .map(|(artist_id, position, credit, join_phrase)| DbAlbumArtist {
                credit,
                join_phrase,
                ..DbAlbumArtist::new(&coven::UuidIds, &album.id, &artist_id, position, now)
            })
            .collect();
        let track_artists = track_credits
            .into_iter()
            .flat_map(|(track_id, list)| {
                list.into_iter().map(move |(artist_id, position, credit, join_phrase)| {
                    DbTrackArtist {
                        credit,
                        join_phrase,
                        ..DbTrackArtist::new(&coven::UuidIds, &track_id, &artist_id, position, now)
                    }
                })
            })
            .collect();
        let track_updates = tracks.into_iter().map(|track| (track.id.clone(), track)).collect();
        Ok(ReleaseMetadataRows {
            album,
            release,
            track_updates,
            picked: credits.iter().map(|artist| artist.id.clone()).collect(),
            credits,
            album_artists,
            track_artists,
        })
    }

    /// Save an edit of `release_id` applying a catalog that credits the
    /// roles and works `links` hold, as a release editor's save does: its
    /// album, pressing, tracks and credits as they stand, and `links` in
    /// place of every role credit and work link the release had.
    #[cfg(desktop)]
    pub async fn apply_release_links_for_test(
        &self,
        release_id: &str,
        links: crate::test_fixtures::library::LibraryRows,
    ) -> Result<(), super::ReleaseEditSaveError> {
        let metadata = self.release_metadata_for_test(release_id).await?;
        let mediums = self.get_release_mediums(release_id).await?;
        let mut artists = metadata.credits;
        let linked = links
            .release_artist_roles
            .iter()
            .map(|role| &role.artist_id)
            .chain(links.track_artist_roles.iter().map(|role| &role.artist_id))
            .chain(links.work_artists.iter().map(|link| &link.artist_id));
        for artist_id in linked {
            if !artists.iter().any(|artist| artist.id == *artist_id) {
                artists.push(self.find_artist_by_id(artist_id).await?.expect("a linked artist"));
            }
        }
        let picked = artists.iter().map(|artist| artist.id.clone()).collect();
        let rows = ReleaseRows {
            album: metadata.album,
            release: metadata.release,
            mediums,
            tracks: metadata
                .track_updates
                .into_iter()
                .map(|(_, track)| TrackRow {
                    id: track.id,
                    release_id: track.release_id,
                    title: track.title,
                    medium_id: track.medium_id,
                    side: track.side,
                    track_number: track.track_number,
                    stated_length_ms: track.stated_length_ms,
                    created_at: track.created_at,
                })
                .collect(),
            artists,
            album_artists: metadata.album_artists,
            track_artists: metadata.track_artists,
            work_graph: WorkGraphRows {
                works: links.works,
                work_artists: links.work_artists,
                work_parts: links.work_parts,
                track_works: links.track_works,
            },
            release_artist_roles: links.release_artist_roles,
            track_artist_roles: links.track_artist_roles,
        };
        self.save_release_edit_atomic(
            ReleaseEditPlan {
                change: None,
                write: ReleaseEditWrite::Applied { rows, picked },
                album_fields: AlbumFields::Written,
            },
            super::GonePicks::Refuse,
            |_| Ok(Ok(())),
            |_| Ok(None),
            None,
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .await
    }

    /// The release a person's metadata edit of `album_id` saves through: its
    /// first.
    async fn edited_release_of_for_test(&self, album_id: &str) -> Result<String, DbError> {
        let releases = self.get_releases_for_album(album_id).await?;
        Ok(releases.into_iter().next().expect("the album holds a release").id)
    }

    /// Rename `album_id` as a person's metadata edit of its first release
    /// does.
    #[cfg(desktop)]
    pub async fn rename_album_for_test(
        &self,
        album_id: &str,
        title: &str,
    ) -> Result<(), crate::ReleaseEditSaveError> {
        let release = self.edited_release_of_for_test(album_id).await?;
        self.save_release_metadata_for_test(&release, |rows| rows.album.title = title.to_string())
            .await
    }

    /// Credit `album_id` to the library artists `artists`, in that order, as
    /// a person's metadata edit of its first release does, picking each.
    #[cfg(desktop)]
    pub async fn credit_album_for_test(
        &self,
        album_id: &str,
        artists: &[&str],
    ) -> Result<(), crate::ReleaseEditSaveError> {
        let release = self.edited_release_of_for_test(album_id).await?;
        let mut credited = Vec::new();
        for artist in artists {
            credited.push(self.find_artist_by_id(artist).await?.expect("a library artist"));
        }
        let now = self.store.now_utc();
        self.save_release_metadata_for_test(&release, |rows| {
            rows.album_artists = artists
                .iter()
                .zip(0..)
                .map(|(artist, position)| {
                    DbAlbumArtist::new(&coven::UuidIds, album_id, artist, position, now)
                })
                .collect();
            for artist in credited {
                if !rows.picked.contains(&artist.id) {
                    rows.picked.push(artist.id.clone());
                    rows.credits.push(artist);
                }
            }
        })
        .await
    }

    /// Rename `track_id` as a person's metadata edit of its release does.
    #[cfg(desktop)]
    pub async fn rename_track_for_test(
        &self,
        track_id: &str,
        title: &str,
    ) -> Result<(), crate::ReleaseEditSaveError> {
        let track = self.find_track_by_id(track_id).await?.expect("the track to rename");
        self.save_release_metadata_for_test(&track.release_id, |rows| {
            for (id, track) in &mut rows.track_updates {
                if id == track_id {
                    track.title = title.to_string();
                }
            }
        })
        .await
    }
}

/// One credit as a test writes it back: the artist, its place, the other
/// printed name it prints if any, and what follows it.
type CreditForTest = (String, u32, Option<crate::credits::CreditName>, Option<String>);

/// One credit as a test writes it back, from the columns of a credit row
/// and the other printed name it joins.
fn credit_for_test(
    artist_id: String,
    position: u32,
    name_key: Option<String>,
    name: Option<String>,
    join_phrase: Option<String>,
) -> Result<CreditForTest, DbError> {
    let credit = super::read::joined_credit_name(name_key, name)
        .map_err(|reason| DbError::unreadable("a credit's name", reason))?;
    Ok((artist_id, position, credit, join_phrase))
}

impl crate::test_fixtures::library::LibraryRows {
    /// The parts of each release's pressing, in order: ids from `ids` for the
    /// labels a write may make and each row of its lists, at `now`.
    pub fn pressing_parts(
        &self,
        ids: &dyn coven::IdSource,
        now: time::UtcDateTime,
    ) -> Vec<PressingParts> {
        self.releases
            .iter()
            .map(|release| PressingParts::new(ids, now, &release.pressing))
            .collect()
    }

    /// Write the rows inside `conn`'s write through the records' own row
    /// inserts, each kind in the order this lists them; `pressing` holds
    /// [`Self::pressing_parts`].
    pub fn write_on(
        &self,
        conn: &CovenWrite<'_>,
        pressing: &[PressingParts],
    ) -> Result<(), IdentityWriteError> {
        for artist in &self.artists {
            insert_artist_row(conn, artist)?;
        }
        for album in &self.albums {
            insert_album_row(conn, album)?;
        }
        for credit in &self.album_artists {
            insert_album_artist_row(conn, credit)?;
        }
        for (release, pressing) in self.releases.iter().zip(pressing) {
            insert_release_row(conn, release, &pressing_write(pressing))?;
        }
        for medium in &self.mediums {
            insert_release_medium_row(conn, medium)?;
        }
        for track in &self.tracks {
            insert_track_row(conn, track)?;
        }
        for credit in &self.track_artists {
            insert_track_artist_row(conn, credit)?;
        }
        for work in &self.works {
            insert_work_row(conn, work)?;
        }
        for link in &self.work_artists {
            insert_work_artist_row(conn, link)?;
        }
        for part in &self.work_parts {
            insert_work_part_row(conn, part)?;
        }
        for link in &self.track_works {
            insert_track_work_row(conn, link)?;
        }
        for role in &self.release_artist_roles {
            insert_release_artist_role_row(conn, role)?;
        }
        for role in &self.track_artist_roles {
            insert_track_artist_role_row(conn, role)?;
        }
        Ok(())
    }
}
