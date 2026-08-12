use super::*;

impl Database {
    /// Insert one or more `release_identities` rows for an existing
    /// release. Idempotent at the PK (release_id, source) — duplicates
    /// surface as unique-violation errors. Used for setting identity
    /// outside of the atomic import path.
    pub async fn insert_release_identities(
        &self,
        release_id: &str,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let identities = identities.to_vec();
        let now = self.inner.clock.now().to_rfc3339();
        let ids = Arc::clone(&self.inner.ids);
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            for identity in &identities {
                insert_release_identity_row(&sql, &release_id, identity, ids.new_id(), &reg, &now)?;
            }
            Ok(())
        })
        .await
    }

    /// All identity rows for a release. Empty if the release has no
    /// `release_identities` rows (Unknown identity).
    pub async fn get_release_identities(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseIdentity>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| get_release_identities_on(&sql, &release_id))
            .await
    }

    /// Look up an album by Exact `release_identities` rows. Returns the
    /// first album that has a release with an identity row matching any
    /// of `identities` on `(source, source_release_id)`, ignoring rows
    /// that belong to `exclude_release_ids`. Approximate identities
    /// (`source_release_id = None`) are ignored — they're group-only
    /// claims, not pressing-level claims.
    ///
    /// Used for the per-pressing rejection step of import dedup: a
    /// duplicate is a release whose identity points at a specific
    /// pressing already in the library.
    pub async fn find_album_by_identity_release_excluding(
        &self,
        identities: &[crate::import::ReleaseIdentity],
        exclude_release_ids: &[String],
    ) -> Result<Option<DbAlbum>, DbError> {
        let exact_pairs: Vec<(String, String)> = identities
            .iter()
            .filter_map(|id| {
                id.source_release_id
                    .as_deref()
                    .map(|rid| (id.source.as_str().to_string(), rid.to_string()))
            })
            .collect();
        if exact_pairs.is_empty() {
            return Ok(None);
        }
        let exclude_release_ids = exclude_release_ids.to_vec();

        self.read(move |sql| {
            let placeholders = exact_pairs
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");
            let exclude_predicate = if exclude_release_ids.is_empty() {
                String::new()
            } else {
                format!(
                    "AND ri.release_id NOT IN ({})",
                    exclude_release_ids
                        .iter()
                        .map(|_| "?")
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let query = format!(
                r#"
                    SELECT
                        a.id, a.title, a.artist_id, a.year, a.primary_release_id,
                        a.is_compilation, a.created_at
                    FROM albums a
                    JOIN releases r ON r.album_id = a.id
                    JOIN release_identities ri ON ri.release_id = r.id
                    WHERE (ri.source, ri.source_release_id) IN ({placeholders})
                      {exclude_predicate}
                    LIMIT 1
                    "#,
            );
            let mut binds: Vec<&str> =
                Vec::with_capacity(exact_pairs.len() * 2 + exclude_release_ids.len());
            for (source, release_id) in &exact_pairs {
                binds.push(source);
                binds.push(release_id);
            }
            for release_id in &exclude_release_ids {
                binds.push(release_id);
            }
            sql.query_row(
                &query,
                coven::rusqlite::params_from_iter(binds.iter()),
                row_to_album,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Look up an album by `release_identities` group rows. Returns the
    /// first album that has a release with an identity row matching any
    /// of `identities` on `(source, source_group_id)`, ignoring rows that
    /// belong to `exclude_release_ids`. Used for the cross-source merge
    /// step of import dedup (excluding the releases a re-import replaces)
    /// and by `set_identity` (excluding the release whose about-to-be-
    /// replaced identity rows must not match against themselves).
    pub async fn find_album_by_identity_group_excluding(
        &self,
        identities: &[crate::import::ReleaseIdentity],
        exclude_release_ids: &[String],
    ) -> Result<Option<String>, DbError> {
        if identities.is_empty() {
            return Ok(None);
        }

        let pairs: Vec<(String, String)> = identities
            .iter()
            .map(|id| (id.source.as_str().to_string(), id.source_group_id.clone()))
            .collect();
        let exclude_release_ids = exclude_release_ids.to_vec();

        self.read(move |sql| {
            let placeholders = pairs
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");
            let exclude_predicate = if exclude_release_ids.is_empty() {
                String::new()
            } else {
                format!(
                    "AND ri.release_id NOT IN ({})",
                    exclude_release_ids
                        .iter()
                        .map(|_| "?")
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let query = format!(
                r#"
                    SELECT r.album_id
                    FROM releases r
                    JOIN release_identities ri ON ri.release_id = r.id
                    WHERE (ri.source, ri.source_group_id) IN ({placeholders})
                      {exclude_predicate}
                    LIMIT 1
                    "#,
            );
            let mut binds: Vec<&str> =
                Vec::with_capacity(pairs.len() * 2 + exclude_release_ids.len());
            for (source, group_id) in &pairs {
                binds.push(source);
                binds.push(group_id);
            }
            for release_id in &exclude_release_ids {
                binds.push(release_id);
            }
            sql.query_row(
                &query,
                coven::rusqlite::params_from_iter(binds.iter()),
                |row| row.get::<_, String>("album_id"),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Replace `release_identities` rows for `release_id`, update the
    /// release's `metadata_source` / `metadata_source_release_id`, and
    /// move the release between albums when the target differs from the
    /// source.
    ///
    /// Everything below runs in one transaction:
    ///
    /// 1. INSERT the destination album when `new_album` is `Some`,
    ///    plus copies of `current_album_id`'s `album_artists` rows
    ///    (so a fresh album lands fully populated, not a bare row that
    ///    drops the artist links the source already had).
    /// 2. Replace `release_identities` for `release_id`.
    /// 3. UPDATE the release's `album_id` and metadata-source columns.
    /// 4. If the release vacated `current_album_id` (the source), check
    ///    inside the transaction whether any releases remain. None →
    ///    delete the source album. Some → clear `primary_release_id`
    ///    if it pointed at the moved release (read paths fall back to
    ///    the first release).
    ///
    /// The post-move recheck on `current_album_id` closes a TOCTOU
    /// window: a separate writer could have inserted a release into the
    /// source between the manager's pre-flight read and this
    /// transaction. Deciding inside the same transaction prevents the
    /// cascade-delete from removing freshly-arrived releases.
    ///
    /// Metadata columns (pressing fields, album fields, tracks) are
    /// deliberately untouched. Caller decides whether to reseed the
    /// metadata.
    ///
    /// Nothing is done to the archived provider documents: they are keyed by
    /// the *source* release, so re-pointing `metadata_source_release_id` at a
    /// different one already reads a different row. There is no stale payload
    /// to wipe, and the rows this release used may be another candidate's.
    ///
    #[allow(clippy::too_many_arguments)]
    pub async fn set_identity_atomic(
        &self,
        release_id: &str,
        new_identities: &[crate::import::ReleaseIdentity],
        new_metadata_source: crate::db::ReleaseMetadataSource,
        new_metadata_source_release_id: Option<&str>,
        current_album_id: &str,
        target_album_id: &str,
        new_album: Option<&DbAlbum>,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let new_identities = new_identities.to_vec();
        let new_metadata_source = new_metadata_source.as_str().to_string();
        let new_metadata_source_release_id = new_metadata_source_release_id.map(str::to_string);
        let current_album_id = current_album_id.to_string();
        let target_album_id = target_album_id.to_string();
        let new_album = new_album.cloned();
        let now_dt = self.inner.clock.now();
        let now = now_dt.to_rfc3339();
        let ids = Arc::clone(&self.inner.ids);

        self.call_sql(move |sql| {
            let tx = &sql;
            // One HLC stamp for every synced row this transaction touches.
            let reg = sql.stamp();

            // 1. Insert the destination album (if brand-new). Must come
            //    before the release UPDATE so the FK on `releases.album_id`
            //    points at an existing row.
            if let Some(album) = &new_album {
                insert_album_row(tx, album, &reg)?;

                // Copy album_artists from the source. Each row gets a fresh
                // PK from the injected id provider, like every other id in the
                // codebase, and is rebound to the new album. The UNIQUE(album_id,
                // artist_id) constraint is satisfied because we're inserting
                // into a different album. If the source is about to be
                // deleted (sole release moved), the SELECT still sees the
                // source rows because the DELETE happens later in the same
                // transaction.
                let source_artists: Vec<(String, i32)> = tx.query(
                    "SELECT artist_id, position FROM album_artists \
                             WHERE album_id = ? ORDER BY position",
                    params![current_album_id],
                    |row| {
                        Ok((
                            row.get::<_, String>("artist_id")?,
                            row.get::<_, i32>("position")?,
                        ))
                    },
                )?;
                for (artist_id, position) in source_artists {
                    let album_artist =
                        DbAlbumArtist::new(&album.id, &artist_id, position, ids.new_id(), now_dt);
                    insert_album_artist_row(tx, &album_artist, &reg)?;
                }
            }

            // 2. Replace identity rows.
            tx.execute(
                "DELETE FROM release_identities WHERE release_id = ?",
                params![release_id],
            )?;
            for identity in &new_identities {
                insert_release_identity_row(tx, &release_id, identity, ids.new_id(), &reg, &now)?;
            }

            // 3. Update release: album, metadata source.
            tx.execute(
                r#"
                    UPDATE releases SET
                        album_id = ?,
                        metadata_source = ?,
                        metadata_source_release_id = ?,
                        _updated_at = ?
                    WHERE id = ?
                    "#,
                params![
                    target_album_id,
                    new_metadata_source,
                    new_metadata_source_release_id,
                    reg,
                    release_id,
                ],
            )?;

            // 4. Source-album cleanup. Only runs when the release actually
            //    moved; same-album updates don't vacate anything. Recheck
            //    inside the transaction (TOCTOU: a writer may have added a
            //    release to the source since the manager's pre-flight read).
            if target_album_id != current_album_id {
                cleanup_album_after_release_removal_on(tx, &current_album_id, &release_id, &reg)?;
            }

            Ok(())
        })
        .await
    }

    /// Check, for each candidate in `checks`, whether the library already
    /// holds the same pressing or the same album (group). Drives the
    /// "in library" badges shown in the identify-pipeline result lists.
    ///
    /// Per check:
    ///
    /// - `release_in_library` is true when a `release_identities` row
    ///   matches `(check.source, check.release_id)` — i.e. an Exact
    ///   identity at this specific pressing. An album-level claim
    ///   leaves `source_release_id` NULL, so the comparison is the
    ///   null-safe `IS`: those rows answer 0, not SQL NULL, and the
    ///   `ORDER BY` still puts a real pressing match first.
    /// - `album_in_library` is true when a `release_identities` row
    ///   matches `(check.source, check.source_group_id)` — i.e. some
    ///   release in the library shares the candidate's group identity.
    ///
    /// `album_title` / `album_id` carry the matched album's display
    /// info. When both flags are true, they describe the album holding
    /// the matching pressing; when only `album_in_library` is true,
    /// they describe the album holding a different release in the same
    /// group.
    pub async fn check_releases_in_library(
        &self,
        checks: &[LibraryCheck],
    ) -> Result<Vec<LibraryStatus>, DbError> {
        let checks = checks.to_vec();
        self.read(move |sql| check_releases_in_library_on(&sql, &checks))
            .await
    }

    pub(crate) fn subscribe_release_library_status(
        &self,
        check: LibraryCheck,
    ) -> coven::LiveQuery<LibraryStatus> {
        self.inner.handle.subscribe(move |sql| {
            let mut statuses = check_releases_in_library_on(&sql, std::slice::from_ref(&check))
                .map_err(CovenError::from)?;
            Ok(statuses
                .pop()
                .expect("one library check produces one library status"))
        })
    }
}

pub(super) fn check_releases_in_library_on(
    sql: &SqlReadContext<'_>,
    checks: &[LibraryCheck],
) -> Result<Vec<LibraryStatus>, DbError> {
    let mut statuses = Vec::with_capacity(checks.len());

    for check in checks {
        let source = check.source.as_str();
        let group_id = check.source_group_id.as_deref();
        let matched = sql
            .query_row(
                r#"
                            SELECT
                                a.id AS album_id,
                                a.title AS album_title,
                                ri.source_release_id IS ? AS release_match
                            FROM albums a
                            JOIN releases r ON r.album_id = a.id
                            JOIN release_identities ri ON ri.release_id = r.id
                            WHERE ri.source = ?
                              AND (
                                  ri.source_release_id = ?
                                  OR (? IS NOT NULL AND ri.source_group_id = ?)
                              )
                            ORDER BY release_match DESC
                            LIMIT 1
                            "#,
                params![
                    check.release_id,
                    source,
                    check.release_id,
                    group_id,
                    group_id
                ],
                |row| {
                    Ok((
                        row.get::<_, String>("album_id")?,
                        row.get::<_, String>("album_title")?,
                        row.get::<_, i64>("release_match")? != 0,
                    ))
                },
            )
            .optional()?;

        let (album_id, album_title, release_in_library) = matched
            .map(|(album_id, album_title, release_match)| {
                (Some(album_id), Some(album_title), release_match)
            })
            .unwrap_or((None, None, false));
        statuses.push(LibraryStatus {
            release_id: check.release_id.clone(),
            release_in_library,
            album_in_library: album_id.is_some(),
            album_title,
            album_id,
        });
    }

    Ok(statuses)
}
