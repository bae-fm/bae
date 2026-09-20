//! Reading and writing a release's records — every catalog's description of
//! it.

use super::*;

impl Database {
    /// Insert one or more record rows for an existing release.
    /// Idempotent at the unique key (release_id, catalog) — duplicates surface
    /// as unique-violation errors. Used for writing records outside of the
    /// atomic import path.
    pub async fn insert_release_records(
        &self,
        release_id: &str,
        records: &[crate::import::ReleaseRecord],
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let records = records.to_vec();
        let now = self.inner.clock.now().to_rfc3339();
        let ids = Arc::clone(&self.inner.ids);
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            for record in &records {
                insert_release_record_row(&sql, &release_id, record, ids.new_id(), &reg, &now)?;
            }
            Ok(())
        })
        .await
    }

    /// Every record for a release. Empty if no catalog describes it.
    pub async fn get_release_records(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseRecord>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| get_release_records_on(&sql, &release_id))
            .await
    }

    /// Look up an album by record rows. Returns the first album that
    /// has a release with a record matching any of `records` on
    /// `(catalog, key)`, ignoring rows that belong to `exclude_release_ids`.
    ///
    /// Used for the per-pressing rejection step of import dedup: a duplicate is
    /// a release whose record points at a specific pressing already in the
    /// library.
    pub async fn find_album_by_record_key_excluding(
        &self,
        records: &[crate::import::ReleaseRecord],
        exclude_release_ids: &[String],
    ) -> Result<Option<DbAlbum>, DbError> {
        let pressing_pairs: Vec<(String, String)> = records
            .iter()
            .filter_map(crate::import::ReleaseRecord::release_ref)
            .map(|release| (release.catalog.as_str().to_string(), release.key.clone()))
            .collect();
        if pressing_pairs.is_empty() {
            return Ok(None);
        }
        let exclude_release_ids = exclude_release_ids.to_vec();

        self.read(move |sql| {
            find_album_by_record_pairs(
                &sql,
                r#"
                    SELECT
                        a.id, a.title, a.artist_id, a.year, a.primary_release_id,
                        a.is_compilation, a.created_at
                    FROM albums a
                    JOIN releases r ON r.album_id = a.id
                    JOIN release_records rr ON rr.release_id = r.id
                "#,
                "CASE WHEN rr.kind = 'pressing' THEN rr.key END",
                &pressing_pairs,
                &exclude_release_ids,
                row_to_album,
            )
        })
        .await
    }

    /// Look up an album by explicit album identities or known pressing parents.
    /// Returns the first album matching `(catalog, album key)`, ignoring rows
    /// that belong to `exclude_release_ids`.
    /// Used for the cross-catalog merge step of import dedup (excluding the
    /// releases a re-import replaces) and by `set_records` (excluding the
    /// release whose about-to-be-replaced records must not match against
    /// themselves).
    pub async fn find_album_by_record_group_excluding(
        &self,
        records: &[crate::import::ReleaseRecord],
        exclude_release_ids: &[String],
    ) -> Result<Option<String>, DbError> {
        let pairs: Vec<(String, String)> = records
            .iter()
            .filter_map(crate::import::ReleaseRecord::album_ref)
            .map(|album| (album.catalog.as_str().to_string(), album.key))
            .collect();
        if pairs.is_empty() {
            return Ok(None);
        }
        let exclude_release_ids = exclude_release_ids.to_vec();

        self.read(move |sql| {
            find_album_by_record_pairs(
                &sql,
                r#"
                    SELECT r.album_id
                    FROM releases r
                    JOIN release_records rr ON rr.release_id = r.id
                "#,
                "CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END",
                &pairs,
                &exclude_release_ids,
                |row| row.get::<_, String>("album_id"),
            )
        })
        .await
    }

    /// Replace a release's record rows, set where its draft was read from —
    /// whether off the files' own tags, and which name read off the object
    /// tied them to the new record — and move the release between albums when
    /// the target differs from the source.
    ///
    /// Everything below runs in one transaction:
    ///
    /// 1. INSERT the destination album when `new_album` is `Some`,
    ///    plus copies of `current_album_id`'s `album_artists` rows
    ///    (so a fresh album lands fully populated, not a bare row that
    ///    drops the artist links the source already had).
    /// 2. Replace the release's records.
    /// 3. UPDATE the release's `album_id`, `draft_from_tags` and
    ///    `identified_by`.
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
    /// the *catalog's* release, so re-pointing the records at a different one
    /// already reads a different row. There is no stale payload to wipe, and
    /// the rows this release used may be another candidate's.
    pub async fn set_records_atomic(
        &self,
        release_id: &str,
        new_records: &[crate::import::ReleaseRecord],
        draft_from_tags: bool,
        identified_by: Option<crate::import::MarkKind>,
        current_album_id: &str,
        target_album_id: &str,
        new_album: Option<&DbAlbum>,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let new_records = new_records.to_vec();
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

            // 2. Replace the records.
            // The old readings' proof belongs to the previous identification.
            // Replacing records supplies no per-value lookup evidence.
            tx.execute(
                "UPDATE release_marks SET corroborated = 0, _updated_at = ? \
                 WHERE release_id = ? AND corroborated = 1",
                params![reg, release_id],
            )?;
            tx.execute(
                "DELETE FROM release_records WHERE release_id = ?",
                params![release_id],
            )?;
            for record in &new_records {
                insert_release_record_row(tx, &release_id, record, ids.new_id(), &reg, &now)?;
            }

            // 3. Update release: album and where its draft was read.
            tx.execute(
                r#"
                    UPDATE releases SET
                        album_id = ?,
                        draft_from_tags = ?,
                        identified_by = ?,
                        _updated_at = ?
                    WHERE id = ?
                    "#,
                params![
                    target_album_id,
                    draft_from_tags,
                    identified_by.map(|kind| kind.as_str()),
                    reg,
                    release_id
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
    /// - `release_in_library` is true when a record
    ///   matches `(check.catalog, check.release_id)` — a record at
    ///   this specific pressing. The `ORDER BY` puts a pressing match
    ///   ahead of a group-only one.
    /// - `album_in_library` is true when a record
    ///   matches `(check.catalog, check.source_group_id)` — i.e. some
    ///   release in the library shares the candidate's group.
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

/// The shared body of the two record lookups above: match `pairs` against
/// `(rr.catalog, {key_expression})`, skip records belonging to
/// `exclude_release_ids`, and map the first row the query produces.
///
/// `select` supplies the projection and joined tables; `key_expression` chooses
/// the pressing or album identity. The pair predicate binds the requested keys.
fn find_album_by_record_pairs<T>(
    sql: &SqlReadContext<'_>,
    select: &str,
    key_expression: &str,
    pairs: &[(String, String)],
    exclude_release_ids: &[String],
    row_to: impl FnOnce(&Row<'_>) -> coven::rusqlite::Result<T>,
) -> Result<Option<T>, DbError> {
    let placeholders = pairs
        .iter()
        .map(|_| "(?, ?)")
        .collect::<Vec<_>>()
        .join(", ");
    let exclude_predicate = if exclude_release_ids.is_empty() {
        String::new()
    } else {
        format!(
            "AND rr.release_id NOT IN ({})",
            exclude_release_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let query = format!(
        "{select}
         WHERE (rr.catalog, {key_expression}) IN ({placeholders})
           {exclude_predicate}
         LIMIT 1"
    );
    let mut binds: Vec<&str> = Vec::with_capacity(pairs.len() * 2 + exclude_release_ids.len());
    for (catalog, key) in pairs {
        binds.push(catalog);
        binds.push(key);
    }
    for release_id in exclude_release_ids {
        binds.push(release_id);
    }
    sql.query_row(
        &query,
        coven::rusqlite::params_from_iter(binds.iter()),
        row_to,
    )
    .optional()
    .map_err(DbError::from)
}

pub(super) fn check_releases_in_library_on(
    sql: &SqlReadContext<'_>,
    checks: &[LibraryCheck],
) -> Result<Vec<LibraryStatus>, DbError> {
    let mut statuses = Vec::with_capacity(checks.len());

    for check in checks {
        let catalog = check.source.as_str();
        let group_id = check.source_group_id.as_deref();
        let matched = sql
            .query_row(
                r#"
                            SELECT
                                a.id AS album_id,
                                a.title AS album_title,
                                (rr.kind = 'pressing' AND rr.key = ?) AS release_match
                            FROM albums a
                            JOIN releases r ON r.album_id = a.id
                            JOIN release_records rr ON rr.release_id = r.id
                            WHERE rr.catalog = ?
                              AND (
                                  (rr.kind = 'pressing' AND rr.key = ?)
                                  OR (? IS NOT NULL AND (CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END) = ?)
                              )
                            ORDER BY release_match DESC
                            LIMIT 1
                            "#,
                params![
                    check.release_id,
                    catalog,
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
