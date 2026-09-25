//! Atomic consolidation of library artists after a persisted import conflict.

use super::*;

impl Database {
    /// Consolidate the two library artists named by a candidate's persisted
    /// identity conflict. The absorbed artist is recorded as merged into the
    /// survivor (`artist_merges`), the survivor takes its source ids and image,
    /// and this device's drafts and conflicts move to the survivor, all in one
    /// database commit; the failed candidate becomes ready again with it.
    /// Neither artist row is deleted and no library credit is rewritten: both
    /// are shared with devices that may be crediting the absorbed artist while
    /// apart, and every read shows the survivor.
    pub async fn merge_import_artist_identity_conflict(
        &self,
        content_hash: &str,
        surviving_artist_id: &str,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let surviving_artist_id = surviving_artist_id.to_string();
        let now = self.inner.clock.now().to_rfc3339();
        let plan = self
            .read(move |sql| {
                plan_artist_identity_merge(&sql, &content_hash, &surviving_artist_id, &now)
            })
            .await?;
        let deleted_image = plan.deleted_image.clone();
        self.inner
            .handle
            .write_with_blobs(
                move |write| {
                    if let Some(image) = deleted_image {
                        write.delete_blob(image);
                    }
                    Ok(())
                },
                move |sql| {
                    let current = plan_artist_identity_merge(
                        &sql,
                        &plan.content_hash,
                        &plan.surviving_artist_id,
                        &plan.now,
                    )
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
            .map(|_| ())
            .map_err(Self::coven_error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtistIdentityMergePlan {
    content_hash: String,
    surviving_artist_id: String,
    absorbed_artist_id: String,
    discogs_artist_id: String,
    musicbrainz_artist_id: String,
    surviving_sort_name: Option<String>,
    move_absorbed_image: bool,
    /// The absorbed artist's image row goes: moved to the survivor, or
    /// superseded by the survivor's own.
    absorbed_image_leaves: bool,
    deleted_image: Option<coven::BlobRef>,
    /// `created_at` for the merge record.
    now: String,
}

fn plan_artist_identity_merge<Q: QueryOne + QueryRows>(
    sql: &Q,
    content_hash: &str,
    surviving_artist_id: &str,
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
    let absorbed_artist_id = if surviving_artist_id == discogs_library_id {
        musicbrainz_library_id.clone()
    } else if surviving_artist_id == musicbrainz_library_id {
        discogs_library_id.clone()
    } else {
        return Err(DbError::Message(format!(
            "artist {surviving_artist_id} is not part of candidate {content_hash}'s identity conflict"
        )));
    };
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
    let surviving_sort_name = if surviving_artist_id == discogs_library_id {
        discogs_artist.sort_name.or(musicbrainz_artist.sort_name)
    } else {
        musicbrainz_artist.sort_name.or(discogs_artist.sort_name)
    };
    let image = |artist_id: &str| {
        sql.query_row(
            "SELECT blob_id, cloud_path FROM artist_images WHERE id = ?",
            [artist_id],
            |row| {
                let blob_id = row.get::<_, String>(0)?;
                let cloud_path = row.get(1)?;
                Ok(crate::sync::image_blob_ref(
                    crate::sync::ARTIST_IMAGES_NAMESPACE,
                    &blob_id,
                    cloud_path,
                ))
            },
        )
        .optional()
        .map_err(DbError::from)
    };
    let surviving_image = image(surviving_artist_id)?;
    let absorbed_image = image(&absorbed_artist_id)?;
    let move_absorbed_image = surviving_image.is_none() && absorbed_image.is_some();
    let absorbed_image_leaves = absorbed_image.is_some();
    let deleted_image = match (surviving_image, absorbed_image) {
        (Some(surviving), Some(absorbed)) if surviving.id != absorbed.id => Some(absorbed),
        _ => None,
    };

    Ok(ArtistIdentityMergePlan {
        content_hash: content_hash.to_string(),
        surviving_artist_id: surviving_artist_id.to_string(),
        absorbed_artist_id,
        discogs_artist_id,
        musicbrainz_artist_id,
        surviving_sort_name,
        move_absorbed_image,
        absorbed_image_leaves,
        deleted_image,
        now: now.to_string(),
    })
}

fn apply_artist_identity_merge(
    sql: &SqlContext<'_, '_>,
    plan: &ArtistIdentityMergePlan,
) -> Result<(), DbError> {
    let reg = sql.stamp();
    // Removing the parent failure also removes the conflict row whose
    // restrictive artist FKs would otherwise prevent consolidation.
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

    if plan.move_absorbed_image {
        sql.execute(
            "INSERT INTO artist_images (id, content_type, file_size, width, height, source, \
                 source_url, cloud_path, _updated_at, created_at, hash, blob_id) \
             SELECT ?, content_type, file_size, width, height, source, source_url, cloud_path, \
                 ?, created_at, hash, blob_id FROM artist_images WHERE id = ?",
            params![plan.surviving_artist_id, reg, plan.absorbed_artist_id],
        )?;
    }

    if plan.absorbed_image_leaves {
        sql.execute(
            "DELETE FROM artist_images WHERE id = ?",
            [&plan.absorbed_artist_id],
        )?;
    }

    // Drafts and conflicts are this device's alone: they move to the survivor.
    // Library credits are shared and stay as written, since another device may
    // be crediting the absorbed artist right now; reads show every credit
    // through `merged_artist_survivors`.
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
        &reg,
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "artist identity merge updated {changed} surviving artists; expected exactly one"
        )));
    }
    // The absorbed artist stays, recorded as merged: a chain that ended at it
    // now ends at the survivor.
    sql.execute(
        "UPDATE artist_merges SET into_artist_id = ?1, _updated_at = ?2 \
         WHERE into_artist_id = ?3",
        params![plan.surviving_artist_id, reg, plan.absorbed_artist_id],
    )?;
    sql.execute(
        "INSERT INTO artist_merges (id, into_artist_id, _updated_at, created_at) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT (id) DO UPDATE SET \
             into_artist_id = excluded.into_artist_id, \
             _updated_at = excluded._updated_at",
        params![
            plan.absorbed_artist_id,
            plan.surviving_artist_id,
            reg,
            plan.now
        ],
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
