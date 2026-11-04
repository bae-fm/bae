//! Atomic consolidation of library artists after a persisted import conflict.

use super::*;

impl ImportTables {
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
        let now = chrono::DateTime::<chrono::Utc>::from(self.database.now()).to_rfc3339();
        let plan = self
            .database
            .read(move |sql| {
                super::artist_identity_choice::require_revision(&sql, &hash, metadata_revision)?;
                plan_artist_identity_merge(&sql, &hash, &now)
            })
            .await?;
        self.commit_artist_identity_merge(plan).await
    }

    /// Commit `plan` once the write finds the library as it was planned
    /// against. The survivor without an image of its own takes the absorbed
    /// artist's, its row referencing the same coven file: nothing is uploaded
    /// again.
    async fn commit_artist_identity_merge(
        &self,
        plan: ArtistIdentityMergePlan,
    ) -> Result<(), DbError> {
        self.database
            .call_sql(move |sql| {
                let current = plan_artist_identity_merge(&sql, &plan.content_hash, &plan.now)?;
                if current != plan {
                    return Err(DbError::Message(format!(
                        "artist identity merge plan for {} changed before commit",
                        plan.content_hash
                    )));
                }
                apply_artist_identity_merge(&sql, &plan)
            })
            .await
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
    /// The coven files of the survivor's and the absorbed artist's images.
    images: (Option<coven::FileId>, Option<coven::FileId>),
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

    let discogs_artist =
        sql.query_row("SELECT * FROM artists WHERE id = ?", [&discogs_library_id], row_to_artist)?;
    let musicbrainz_artist = sql.query_row(
        "SELECT * FROM artists WHERE id = ?",
        [&musicbrainz_library_id],
        row_to_artist,
    )?;
    if !bae_db::artist_source_ids_are_compatible(
        &discogs_artist,
        Some(&discogs_artist_id),
        Some(&musicbrainz_artist_id),
    ) || !bae_db::artist_source_ids_are_compatible(
        &musicbrainz_artist,
        Some(&discogs_artist_id),
        Some(&musicbrainz_artist_id),
    ) {
        return Err(DbError::Message(format!(
            "candidate {content_hash}'s library artists no longer contain only the recorded source IDs"
        )));
    }
    let surviving_sort_name = discogs_artist.sort_name.or(musicbrainz_artist.sort_name);
    let image = |artist_id: &str| {
        bae_db::client::files::image_file_on(sql, &LibraryImageType::Artist, artist_id)
    };
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
    sql: &CovenWrite<'_>,
    plan: &ArtistIdentityMergePlan,
) -> Result<(), DbError> {
    super::artist_identity_choice::record_choice_on(
        sql,
        &plan.content_hash,
        &plan.discogs_artist_id,
        &plan.musicbrainz_artist_id,
        crate::ArtistIdentityConflictChoice::SameArtist,
        &plan.surviving_artist_id,
    )?;
    // Removing the parent failure also removes the conflict row whose
    // stored conflict no longer applies after consolidation.
    crate::tables::import_state::failure_rows::delete_failure_on(sql, &plan.content_hash)?;
    // Another selected candidate can have failed on the same pair: every
    // conflict that becomes one artist after this merge goes, and any that
    // still names a different third artist names the survivor.
    crate::tables::import_state::failure_rows::merge_conflicts_on(
        sql,
        crate::tables::import_state::failure_rows::ConflictIdentity::Artist,
        &plan.absorbed_artist_id,
        &plan.surviving_artist_id,
    )?;

    // Drafts and conflicts are this device's alone: they move to the survivor.
    // Library credits are shared and stay as written, since another device may
    // be crediting the absorbed artist right now; reads show every credit
    // through `artist_shown_as`.
    for (table, unique_columns) in [
        ("import_candidate_album_artist_assignment", &["content_hash"][..]),
        ("import_candidate_track_artist_assignment", &["content_hash", "track_id"][..]),
    ] {
        merge_local_artist_references(
            sql,
            table,
            unique_columns,
            &plan.surviving_artist_id,
            &plan.absorbed_artist_id,
        )?;
    }

    bae_db::client::identity_merges::merge_artist_on(
        sql,
        &bae_db::client::identity_merges::ArtistMerge {
            survivor: &plan.surviving_artist_id,
            absorbed: &plan.absorbed_artist_id,
            discogs_artist_id: Some(&plan.discogs_artist_id),
            musicbrainz_artist_id: Some(&plan.musicbrainz_artist_id),
            sort_name: plan.surviving_sort_name.as_deref(),
            at: &plan.now,
        },
    )
}

fn merge_local_artist_references(
    sql: &CovenWrite<'_>,
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

#[cfg(test)]
#[path = "artist_identity_merge_tests.rs"]
mod tests;
