//! Atomic consolidation of library artists after a persisted import conflict.

use super::*;

impl Database {
    /// Consolidate the two library artists named by a candidate's persisted
    /// identity conflict. Every library and pending-import reference moves in
    /// the same database commit; the failed candidate becomes ready again only
    /// if the absorbed artist has been removed successfully.
    pub async fn merge_import_artist_identity_conflict(
        &self,
        content_hash: &str,
        surviving_artist_id: &str,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let surviving_artist_id = surviving_artist_id.to_string();
        let plan = self
            .read(move |sql| plan_artist_identity_merge(&sql, &content_hash, &surviving_artist_id))
            .await?;
        let deleted_image = plan.deleted_image.clone();
        self.inner
            .handle
            .write_with_blobs(
                move |write| {
                    if let Some(image) = deleted_image {
                        write.delete_blob(crate::sync::image_blob_ref(
                            crate::sync::ARTIST_IMAGES_NAMESPACE,
                            &image.blob_id,
                            image.cloud_path,
                        ));
                    }
                    Ok(())
                },
                move |sql| {
                    let current = plan_artist_identity_merge(
                        &sql,
                        &plan.content_hash,
                        &plan.surviving_artist_id,
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
struct ArtistImageDeletion {
    blob_id: String,
    cloud_path: Option<String>,
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
    deleted_image: Option<ArtistImageDeletion>,
}

fn plan_artist_identity_merge<Q: QueryOne + QueryRows>(
    sql: &Q,
    content_hash: &str,
    surviving_artist_id: &str,
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
    if discogs_artist.discogs_artist_id.as_deref() != Some(discogs_artist_id.as_str()) {
        return Err(DbError::Message(format!(
            "candidate {content_hash}'s Discogs-side artist changed after the conflict was recorded"
        )));
    }
    if musicbrainz_artist.musicbrainz_artist_id.as_deref() != Some(musicbrainz_artist_id.as_str()) {
        return Err(DbError::Message(format!(
            "candidate {content_hash}'s MusicBrainz-side artist changed after the conflict was recorded"
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
                Ok(ArtistImageDeletion {
                    blob_id: row.get(0)?,
                    cloud_path: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(DbError::from)
    };
    let surviving_image = image(surviving_artist_id)?;
    let absorbed_image = image(&absorbed_artist_id)?;
    let move_absorbed_image = surviving_image.is_none() && absorbed_image.is_some();
    let deleted_image = match (surviving_image, absorbed_image) {
        (Some(surviving), Some(absorbed)) if surviving.blob_id != absorbed.blob_id => {
            Some(absorbed)
        }
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
        deleted_image,
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
    // restrictive conflict reference before the absorbed artist is deleted.
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
    sql.execute(
        "UPDATE import_candidate_artist_identity_conflict \
         SET discogs_library_artist_id = ?1 WHERE discogs_library_artist_id = ?2",
        params![plan.surviving_artist_id, plan.absorbed_artist_id],
    )?;
    sql.execute(
        "UPDATE import_candidate_artist_identity_conflict \
         SET musicbrainz_library_artist_id = ?1 WHERE musicbrainz_library_artist_id = ?2",
        params![plan.surviving_artist_id, plan.absorbed_artist_id],
    )?;

    if plan.move_absorbed_image {
        sql.execute(
            "INSERT INTO artist_images (id, content_type, file_size, width, height, source, \
                 source_url, cloud_path, _updated_at, created_at, hash, blob_id) \
             SELECT ?, content_type, file_size, width, height, source, source_url, cloud_path, \
                 ?, created_at, hash, blob_id FROM artist_images WHERE id = ?",
            params![plan.surviving_artist_id, reg, plan.absorbed_artist_id],
        )?;
    }

    sql.execute(
        "UPDATE albums SET artist_id = ?, _updated_at = ? WHERE artist_id = ?",
        params![plan.surviving_artist_id, reg, plan.absorbed_artist_id],
    )?;
    merge_single_artist_membership(
        sql,
        "album_artists",
        "album_id",
        &plan.surviving_artist_id,
        &plan.absorbed_artist_id,
        &reg,
    )?;
    merge_single_artist_membership(
        sql,
        "track_artists",
        "track_id",
        &plan.surviving_artist_id,
        &plan.absorbed_artist_id,
        &reg,
    )?;
    merge_positioned_artist_membership(
        sql,
        "work_artists",
        "work_id",
        &["position"],
        &plan.surviving_artist_id,
        &plan.absorbed_artist_id,
        &reg,
    )?;
    merge_positioned_artist_membership(
        sql,
        "release_artist_roles",
        "release_id",
        &["position", "source"],
        &plan.surviving_artist_id,
        &plan.absorbed_artist_id,
        &reg,
    )?;
    merge_positioned_artist_membership(
        sql,
        "track_artist_roles",
        "track_id",
        &["position", "source"],
        &plan.surviving_artist_id,
        &plan.absorbed_artist_id,
        &reg,
    )?;
    merge_candidate_artist_assignments(
        sql,
        "import_candidate_album_artist_assignment",
        &["content_hash"],
        &plan.surviving_artist_id,
        &plan.absorbed_artist_id,
    )?;
    merge_candidate_artist_assignments(
        sql,
        "import_candidate_track_artist_assignment",
        &["content_hash", "track_id"],
        &plan.surviving_artist_id,
        &plan.absorbed_artist_id,
    )?;

    let changed = sql.execute(
        "UPDATE artists SET discogs_artist_id = ?, musicbrainz_artist_id = ?, \
             sort_name = ?, _updated_at = ? WHERE id = ?",
        params![
            plan.discogs_artist_id,
            plan.musicbrainz_artist_id,
            plan.surviving_sort_name,
            reg,
            plan.surviving_artist_id
        ],
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "artist identity merge updated {changed} surviving artists; expected exactly one"
        )));
    }
    let deleted = sql.execute(
        "DELETE FROM artists WHERE id = ?",
        [&plan.absorbed_artist_id],
    )?;
    if deleted != 1 {
        return Err(DbError::Message(format!(
            "artist identity merge deleted {deleted} absorbed artists; expected exactly one"
        )));
    }
    Ok(())
}

fn merge_single_artist_membership(
    sql: &SqlContext<'_, '_>,
    table: &str,
    parent_column: &str,
    surviving_artist_id: &str,
    absorbed_artist_id: &str,
    reg: &str,
) -> Result<(), DbError> {
    sql.execute(
        &format!(
            "DELETE FROM {table} WHERE artist_id = ?1 AND EXISTS (\
                 SELECT 1 FROM {table} survivor \
                 WHERE survivor.{parent_column} = {table}.{parent_column} \
                   AND survivor.artist_id = ?2)"
        ),
        params![absorbed_artist_id, surviving_artist_id],
    )?;
    sql.execute(
        &format!("UPDATE {table} SET artist_id = ?1, _updated_at = ?2 WHERE artist_id = ?3"),
        params![surviving_artist_id, reg, absorbed_artist_id],
    )?;
    Ok(())
}

fn merge_positioned_artist_membership(
    sql: &SqlContext<'_, '_>,
    table: &str,
    parent_column: &str,
    unique_columns: &[&str],
    surviving_artist_id: &str,
    absorbed_artist_id: &str,
    reg: &str,
) -> Result<(), DbError> {
    let equal_unique_columns = unique_columns
        .iter()
        .map(|column| format!("survivor.{column} = {table}.{column}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    sql.execute(
        &format!(
            "DELETE FROM {table} WHERE artist_id = ?1 AND EXISTS (\
                 SELECT 1 FROM {table} survivor \
                 WHERE survivor.{parent_column} = {table}.{parent_column} \
                   AND {equal_unique_columns} AND survivor.artist_id = ?2)"
        ),
        params![absorbed_artist_id, surviving_artist_id],
    )?;
    sql.execute(
        &format!("UPDATE {table} SET artist_id = ?1, _updated_at = ?2 WHERE artist_id = ?3"),
        params![surviving_artist_id, reg, absorbed_artist_id],
    )?;
    Ok(())
}

fn merge_candidate_artist_assignments(
    sql: &SqlContext<'_, '_>,
    table: &str,
    parent_columns: &[&str],
    surviving_artist_id: &str,
    absorbed_artist_id: &str,
) -> Result<(), DbError> {
    let same_parent = parent_columns
        .iter()
        .map(|column| format!("survivor.{column} = {table}.{column}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    sql.execute(
        &format!(
            "DELETE FROM {table} WHERE artist_id = ?1 AND EXISTS (\
                 SELECT 1 FROM {table} survivor \
                 WHERE {same_parent} AND survivor.artist_id = ?2)"
        ),
        params![absorbed_artist_id, surviving_artist_id],
    )?;
    sql.execute(
        &format!("UPDATE {table} SET artist_id = ?1 WHERE artist_id = ?2"),
        params![surviving_artist_id, absorbed_artist_id],
    )?;
    Ok(())
}
