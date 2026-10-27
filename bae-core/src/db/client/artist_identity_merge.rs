//! Atomic consolidation of library artists after a persisted import conflict.

use super::*;

impl Database {
    /// Consolidate the two library artists named by a candidate's persisted
    /// identity conflict. The absorbed artist is recorded as merged into the
    /// survivor (`artist_merges`), the survivor takes its source ids and image,
    /// and this device's drafts and conflicts move to the survivor, all in one
    /// database commit; the candidate's recorded conflict is dismissed with it.
    /// Neither artist row is deleted and no library credit is rewritten: both
    /// are shared with devices that may be crediting the absorbed artist while
    /// apart, and every read shows the survivor.
    pub(super) async fn merge_import_artist_identity_choice(
        &self,
        content_hash: &str,
        metadata_revision: u64,
    ) -> Result<(), DbError> {
        let hash = content_hash.to_string();
        let now = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now()).to_rfc3339();
        let plan = self
            .read(move |sql| {
                super::artist_identity_choice::require_revision(&sql, &hash, metadata_revision)?;
                plan_artist_identity_merge(&sql, &hash, &now)
            })
            .await?;
        self.commit_artist_identity_merge(plan).await
    }

    async fn commit_artist_identity_merge(
        &self,
        plan: ArtistIdentityMergePlan,
    ) -> Result<(), DbError> {
        let moved = if let (None, Some(_)) = &plan.images {
            let reference = self
                .file_ref("artist_images", &plan.absorbed_artist_id)
                .await?;
            let bytes = self
                .read_file(&reference)
                .await
                .map_err(DbError::FileRead)?;
            Some((reference, bytes))
        } else {
            None
        };
        let reference = moved.as_ref().map(|(reference, _)| reference.clone());
        // The absorbed artist's image row goes; an upload of it still queued
        // here is abandoned by the merge the person chose, so its record is
        // dismissed.
        let abandoned = match &plan.images.1 {
            Some(_) => {
                self.own_abandonments(&[("artist_images", plan.absorbed_artist_id.clone())])
                    .await?
            }
            None => Vec::new(),
        };
        // The survivor's copied row is the one these bytes attach to.
        let survivor = plan.surviving_artist_id.clone();
        self.write_with_files(
            move |write| {
                if let Some((_, bytes)) = moved {
                    write.put_file(crate::sync::ARTIST_IMAGES_NAMESPACE, survivor, bytes);
                }
                Ok(())
            },
            move |sql| {
                if let Some(reference) = reference {
                    sql.validate_file_ref(&reference)?;
                }
                let current = plan_artist_identity_merge(&sql, &plan.content_hash, &plan.now)
                    .map_err(CovenError::from)?;
                if current != plan {
                    return Err(CovenError::from(DbError::Message(format!(
                        "artist identity merge plan for {} changed before commit",
                        plan.content_hash
                    ))));
                }
                apply_artist_identity_merge(&sql, &plan).map_err(CovenError::from)
            },
        )
        .await
        .map_err(Self::coven_error)?;
        self.dismiss_own_abandonments(&abandoned).await;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtistIdentityMergePlan {
    content_hash: String,
    surviving_artist_id: String,
    metadata_revision: i64,
    absorbed_artist_id: String,
    discogs_artist_id: String,
    musicbrainz_artist_id: String,
    surviving_sort_name: Option<String>,
    /// The survivor's and absorbed artist's image versions.
    images: (Option<String>, Option<String>),
    /// `created_at` for the merge record.
    now: String,
}

fn plan_artist_identity_merge<Q: QueryOne + QueryRows>(
    sql: &Q,
    content_hash: &str,
    now: &str,
) -> Result<ArtistIdentityMergePlan, DbError> {
    let conflict = sql
        .query_row(
            "SELECT discogs_artist_id, musicbrainz_artist_id, \
                    discogs_library_artist_id, musicbrainz_library_artist_id \
             FROM import_candidate_artist_identity_conflict WHERE content_hash = ?",
            [content_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((
        discogs_artist_id,
        musicbrainz_artist_id,
        discogs_library_id,
        musicbrainz_library_id,
    )) = conflict
    else {
        return Err(DbError::Message(format!(
            "candidate {content_hash} has no artist identity conflict to merge"
        )));
    };
    let surviving_artist_id = discogs_library_id.as_str();
    let absorbed_artist_id = musicbrainz_library_id.clone();
    if surviving_artist_id == absorbed_artist_id {
        return Err(DbError::Message(format!(
            "candidate {content_hash}'s artist identity conflict names one library artist twice"
        )));
    }

    let discogs_artist = sql.query_row(
        "SELECT * FROM artists WHERE id = ?",
        [&discogs_library_id],
        row_to_artist,
    )?;
    let musicbrainz_artist = sql.query_row(
        "SELECT * FROM artists WHERE id = ?",
        [&musicbrainz_library_id],
        row_to_artist,
    )?;
    if !crate::import::artist_source_ids_are_compatible(
        &discogs_artist,
        Some(&discogs_artist_id),
        Some(&musicbrainz_artist_id),
    ) || !crate::import::artist_source_ids_are_compatible(
        &musicbrainz_artist,
        Some(&discogs_artist_id),
        Some(&musicbrainz_artist_id),
    ) {
        return Err(DbError::Message(format!(
            "candidate {content_hash}'s library artists no longer contain only the recorded source IDs"
        )));
    }
    let surviving_sort_name = discogs_artist.sort_name.or(musicbrainz_artist.sort_name);
    let image =
        |artist_id: &str| super::files::image_version_on(sql, &LibraryImageType::Artist, artist_id);
    let surviving_image = image(surviving_artist_id)?;
    let absorbed_image = image(&absorbed_artist_id)?;

    let metadata_revision = sql.query_row(
        "SELECT metadata_revision FROM import_candidate_state WHERE content_hash = ?",
        [content_hash],
        |row| row.get(0),
    )?;
    Ok(ArtistIdentityMergePlan {
        metadata_revision,
        content_hash: content_hash.to_string(),
        surviving_artist_id: surviving_artist_id.to_string(),
        absorbed_artist_id,
        discogs_artist_id,
        musicbrainz_artist_id,
        surviving_sort_name,
        images: (surviving_image, absorbed_image),
        now: now.to_string(),
    })
}

fn apply_artist_identity_merge(
    sql: &SqlContext<'_, '_>,
    plan: &ArtistIdentityMergePlan,
) -> Result<(), DbError> {
    super::artist_identity_choice::record_choice_on(
        sql,
        &plan.content_hash,
        &plan.discogs_artist_id,
        &plan.musicbrainz_artist_id,
        crate::import::ArtistIdentityConflictChoice::SameArtist,
        &plan.surviving_artist_id,
    )?;
    // Removing the parent failure also removes the conflict row whose
    // stored conflict no longer applies after consolidation.
    sql.execute(
        "DELETE FROM import_candidate_failure WHERE content_hash = ?",
        [&plan.content_hash],
    )?;
    // Another selected candidate can have failed on the same pair. Clear every
    // conflict that becomes one artist after this merge, then retarget any
    // conflict that still names a different third artist. This removes every
    // conflict that would otherwise still name two artists that are now one.
    sql.execute(
        "DELETE FROM import_candidate_failure WHERE content_hash IN (\
             SELECT content_hash FROM import_candidate_artist_identity_conflict \
             WHERE (discogs_library_artist_id = ?1 OR musicbrainz_library_artist_id = ?1) \
               AND CASE WHEN discogs_library_artist_id = ?1 THEN ?2 \
                        ELSE discogs_library_artist_id END \
                   = CASE WHEN musicbrainz_library_artist_id = ?1 THEN ?2 \
                          ELSE musicbrainz_library_artist_id END)",
        params![plan.absorbed_artist_id, plan.surviving_artist_id],
    )?;
    for column in ["discogs_library_artist_id", "musicbrainz_library_artist_id"] {
        sql.execute(
            &format!(
                "UPDATE import_candidate_artist_identity_conflict \
                 SET {column} = ?1 WHERE {column} = ?2"
            ),
            params![plan.surviving_artist_id, plan.absorbed_artist_id],
        )?;
    }

    if matches!(&plan.images, (None, Some(_))) {
        sql.execute(
            "INSERT INTO artist_images (id, content_type, file_size, width, height, source, \
                 source_url, created_at, revision) \
             SELECT ?, content_type, file_size, width, height, source, source_url, \
                 created_at, revision FROM artist_images WHERE id = ?",
            params![plan.surviving_artist_id, plan.absorbed_artist_id],
        )?;
    }

    if plan.images.1.is_some() {
        sql.execute(
            "DELETE FROM artist_images WHERE id = ?",
            [&plan.absorbed_artist_id],
        )?;
    }

    // Drafts and conflicts are this device's alone: they move to the survivor.
    // Library credits are shared and stay as written, since another device may
    // be crediting the absorbed artist right now; reads show every credit
    // through `artist_shown_as`.
    for (table, unique_columns) in [
        (
            "import_candidate_album_artist_assignment",
            &["content_hash"][..],
        ),
        (
            "import_candidate_track_artist_assignment",
            &["content_hash", "track_id"][..],
        ),
    ] {
        merge_local_artist_references(
            sql,
            table,
            unique_columns,
            &plan.surviving_artist_id,
            &plan.absorbed_artist_id,
        )?;
    }

    let changed = update_artist_external_ids_row(
        sql,
        &plan.surviving_artist_id,
        Some(&plan.discogs_artist_id),
        Some(&plan.musicbrainz_artist_id),
        plan.surviving_sort_name.as_deref(),
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "artist identity merge updated {changed} surviving artists; expected exactly one"
        )));
    }
    // The absorbed artist stays, recorded as merged: a chain that ended at it
    // now ends at the survivor.
    sql.execute(
        "UPDATE artist_merges SET into_artist_id = ?1 \
         WHERE into_artist_id = ?2",
        params![plan.surviving_artist_id, plan.absorbed_artist_id],
    )?;
    sql.execute(
        "INSERT INTO artist_merges (id, into_artist_id, created_at) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT (id) DO UPDATE SET \
             into_artist_id = excluded.into_artist_id",
        params![plan.absorbed_artist_id, plan.surviving_artist_id, plan.now],
    )?;
    Ok(())
}

fn merge_local_artist_references(
    sql: &SqlContext<'_, '_>,
    table: &str,
    unique_columns: &[&str],
    surviving_artist_id: &str,
    absorbed_artist_id: &str,
) -> Result<(), DbError> {
    let same_reference = unique_columns
        .iter()
        .map(|column| format!("survivor.{column} = {table}.{column}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    sql.execute(
        &format!(
            "DELETE FROM {table} WHERE artist_id = ?1 AND EXISTS (\
                 SELECT 1 FROM {table} survivor \
                 WHERE {same_reference} AND survivor.artist_id = ?2)"
        ),
        params![absorbed_artist_id, surviving_artist_id],
    )?;
    sql.execute(
        &format!("UPDATE {table} SET artist_id = ?1 WHERE artist_id = ?2"),
        params![surviving_artist_id, absorbed_artist_id],
    )?;
    Ok(())
}
