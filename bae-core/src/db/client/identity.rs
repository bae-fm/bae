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
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            let conn = sql.connection();
            for identity in &identities {
                insert_release_identity_row(conn, &release_id, identity, &reg, &now)?;
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
        self.call(move |conn| get_release_identities_on(conn, &release_id))
            .await
    }

    /// Look up an album by Exact `release_identities` rows. Returns the
    /// first album that has a release with an identity row matching any
    /// of `identities` on `(source, source_release_id)`. Approximate
    /// identities (`source_release_id = None`) are ignored — they're
    /// group-only claims, not pressing-level claims.
    ///
    /// Used for the per-pressing rejection step of import dedup: a
    /// duplicate is a release whose identity points at a specific
    /// pressing already in the library.
    pub async fn find_album_by_identity_release(
        &self,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<Option<DbAlbum>, DbError> {
        self.find_album_by_identity_release_excluding(identities, &[])
            .await
    }

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

        self.call(move |conn| {
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
            let sql = format!(
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
            conn.query_row(
                &sql,
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
    /// of `identities` on `(source, source_group_id)`. Used for the
    /// cross-source merge step of import dedup.
    pub async fn find_album_by_identity_group(
        &self,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<Option<String>, DbError> {
        self.find_album_by_identity_group_match(identities, &[])
            .await
    }

    /// Same as `find_album_by_identity_group`, but ignores rows belonging to
    /// `exclude_release_id`. Used by `set_identity` to look for an album
    /// the release would fit into without matching against the release's
    /// own existing (about-to-be-replaced) identity rows.
    pub async fn find_album_by_identity_group_excluding(
        &self,
        identities: &[crate::import::ReleaseIdentity],
        exclude_release_id: &str,
    ) -> Result<Option<String>, DbError> {
        self.find_album_by_identity_group_match(identities, &[exclude_release_id.to_string()])
            .await
    }

    pub async fn find_album_by_identity_group_excluding_many(
        &self,
        identities: &[crate::import::ReleaseIdentity],
        exclude_release_ids: &[String],
    ) -> Result<Option<String>, DbError> {
        self.find_album_by_identity_group_match(identities, exclude_release_ids)
            .await
    }

    async fn find_album_by_identity_group_match(
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

        self.call(move |conn| {
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
            let sql = format!(
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
            conn.query_row(
                &sql,
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
    ///    delete the source album. Some → repair `primary_release_id`
    ///    if it pointed at the moved release.
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
    /// Returns `SetIdentityOutcome::source_album_deleted` so the caller
    /// knows whether to emit `AlbumRemoved` or `AlbumUpdated` for the
    /// source.
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
        new_metadata: &[DbReleaseMetadata],
    ) -> Result<SetIdentityOutcome, DbError> {
        let release_id = release_id.to_string();
        let new_identities = new_identities.to_vec();
        let new_metadata_source = new_metadata_source.as_str().to_string();
        let new_metadata_source_release_id = new_metadata_source_release_id.map(str::to_string);
        let current_album_id = current_album_id.to_string();
        let target_album_id = target_album_id.to_string();
        let new_album = new_album.cloned();
        let new_metadata = new_metadata.to_vec();
        let now_dt = self.inner.clock.now();
        let now = now_dt.to_rfc3339();

        self.call_sql(move |sql| {
            let tx = sql.connection();
            // One HLC stamp for every synced row this transaction touches.
            let reg = sql.stamp();

            // 1. Insert the destination album (if brand-new). Must come
            //    before the release UPDATE so the FK on `releases.album_id`
            //    points at an existing row.
            if let Some(album) = &new_album {
                insert_album_row(tx, album, &reg)?;

                // Copy album_artists from the source. Each row gets a fresh
                // PK (generated in Rust to match the rest of the codebase)
                // and is rebound to the new album. The UNIQUE(album_id,
                // artist_id) constraint is satisfied because we're inserting
                // into a different album. If the source is about to be
                // deleted (sole release moved), the SELECT still sees the
                // source rows because the DELETE happens later in the same
                // transaction.
                let source_artists: Vec<(String, i32)> = {
                    let mut stmt = tx.prepare(
                        "SELECT artist_id, position FROM album_artists \
                                 WHERE album_id = ? ORDER BY position",
                    )?;
                    let rows = stmt.query_map(params![current_album_id], |row| {
                        Ok((
                            row.get::<_, String>("artist_id")?,
                            row.get::<_, i32>("position")?,
                        ))
                    })?;
                    rows.collect::<coven::rusqlite::Result<Vec<_>>>()?
                };
                for (artist_id, position) in source_artists {
                    let album_artist = DbAlbumArtist::new(
                        &album.id,
                        &artist_id,
                        position,
                        uuid::Uuid::new_v4().to_string(),
                        now_dt,
                    );
                    insert_album_artist_row(tx, &album_artist, &reg)?;
                }
            }

            // 2. Replace identity rows.
            tx.execute(
                "DELETE FROM release_identities WHERE release_id = ?",
                params![release_id],
            )?;
            for identity in &new_identities {
                insert_release_identity_row(tx, &release_id, identity, &reg, &now)?;
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

            // 4. Replace cached source payload. Always wipe first — Unknown
            //    drops to file_tags (`new_metadata` empty) and the prior
            //    MB/Discogs JSON has no business sticking around. For
            //    Exact/Approximate, the caller hands us the freshly-fetched
            //    payload (matching the new `metadata_source_release_id`) so
            //    a later re-projection can replay the seed without
            //    divergence.
            tx.execute(
                "DELETE FROM release_metadata WHERE release_id = ?",
                params![release_id],
            )?;
            for meta in &new_metadata {
                insert_release_metadata_row(tx, meta)?;
            }

            // 5. Source-album cleanup. Only runs when the release actually
            //    moved; same-album updates don't vacate anything.
            let mut source_album_deleted = false;
            if target_album_id != current_album_id {
                // Recheck inside the transaction: how many releases does the
                // source album hold now (after the UPDATE above)?
                let remaining: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM releases WHERE album_id = ?",
                    params![current_album_id],
                    |row| row.get(0),
                )?;

                if remaining == 0 {
                    // No releases left → delete the album. There can be no
                    // imports to clear because the only way `releases` is
                    // empty is if every prior release left the album, and
                    // imports reference releases (not albums) — moving a
                    // release elsewhere keeps the import row pointing at the
                    // same release, just with a different `album_id`.
                    tx.execute("DELETE FROM albums WHERE id = ?", params![current_album_id])?;
                    source_album_deleted = true;
                } else {
                    // Album survives. If its `primary_release_id` pointed at
                    // the moved release, repoint it at the oldest remaining
                    // release (matching the "first release" fallback used
                    // elsewhere in the read path).
                    let dangling: Option<String> = tx
                        .query_row(
                            "SELECT primary_release_id FROM albums \
                                 WHERE id = ? AND primary_release_id = ?",
                            params![current_album_id, release_id],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .optional()?
                        .flatten();

                    if dangling.is_some() {
                        let new_primary: Option<String> = tx
                            .query_row(
                                "SELECT id FROM releases \
                                     WHERE album_id = ? \
                                     ORDER BY created_at ASC, id ASC \
                                     LIMIT 1",
                                params![current_album_id],
                                |row| row.get::<_, String>(0),
                            )
                            .optional()?;

                        tx.execute(
                            "UPDATE albums SET primary_release_id = ?, _updated_at = ? \
                                 WHERE id = ?",
                            params![new_primary, reg, current_album_id],
                        )?;
                    }
                }
            }

            Ok(SetIdentityOutcome {
                source_album_deleted,
            })
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
    ///   identity at this specific pressing.
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

        self.call(move |conn| {
            let mut statuses = Vec::with_capacity(checks.len());

            for check in &checks {
                let source = check.source.as_str();
                let group_id = check.source_group_id.as_deref();
                let matched = conn
                    .query_row(
                        r#"
                            SELECT
                                a.id AS album_id,
                                a.title AS album_title,
                                ri.source_release_id = ? AS release_match
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
        })
        .await
    }
}
