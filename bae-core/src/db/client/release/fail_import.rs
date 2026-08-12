use super::*;

/// The exact rows [`Database::fail_import_and_delete_release`] deletes for a
/// rolled-back release, plus the host-provided image blobs those rows carry, so
/// the atomic write that deletes them can declare the blob deletions coven turns
/// into durable local-cleanup intents.
pub(super) struct FailImportDeletion {
    pub(super) release_id: String,
    pub(super) album_id: String,
    /// The album is removed with the release iff it holds no other release.
    pub(super) delete_album: bool,
    /// Works whose only tracks were this release's — unreferenced once it is gone.
    pub(super) orphaned_work_ids: Vec<String>,
    /// Artists referenced only within this release's deleted subtree.
    pub(super) orphaned_artist_ids: Vec<String>,
    /// `(namespace, blob_id, cloud_path)` for the release's cover and each swept
    /// artist's image — the host-provided blobs the delete orphans.
    pub(super) image_blobs: Vec<(&'static str, String, Option<String>)>,
    /// The release's `release_files` rows, whose external-file registrations the
    /// delete must drop. Captured here because the registration is keyed by the
    /// row, and the rows are gone by the time the write commits.
    pub(super) file_ids: Vec<String>,
}

/// Determine, without mutating, exactly what a failed-import rollback of
/// `release_id` deletes and which host-provided image blobs it orphans. Mirrors
/// the delete's reachability: a work orphans when this release held its only
/// tracks; an artist orphans when every album, link, or role that names it lies
/// inside the deleted subtree — this release, its album if the album holds no
/// other release, and the orphaned works.
pub(super) fn plan_fail_import_deletion(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<FailImportDeletion, DbError> {
    let album_id: String = sql
        .query_row(
            "SELECT album_id FROM releases WHERE id = ?",
            params![release_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DbError::Message(format!("release not found: {release_id}")))?;

    let delete_album: bool = sql.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM releases WHERE album_id = ?1 AND id != ?2)",
        params![album_id, release_id],
        |row| row.get(0),
    )?;

    // Every artist this release/album references — the candidates the sweep
    // considers.
    let mut candidate_artist_ids: Vec<String> = sql.query(
        "SELECT artist_id FROM album_artists WHERE album_id = ?1
             UNION
             SELECT artist_id FROM track_artists
               WHERE track_id IN (SELECT id FROM tracks WHERE release_id = ?2)
             UNION
             SELECT artist_id FROM release_artist_roles WHERE release_id = ?2
             UNION
             SELECT artist_id FROM track_artist_roles
               WHERE track_id IN (SELECT id FROM tracks WHERE release_id = ?2)
             UNION
             SELECT artist_id FROM albums WHERE id = ?1 AND artist_id IS NOT NULL",
        params![album_id, release_id],
        |row| row.get(0),
    )?;

    // The work-graph component this release's tracks touch.
    let candidate_work_ids: Vec<String> = sql.query(
        "WITH RECURSIVE component(work_id) AS (
                 SELECT tw.work_id FROM track_works tw
                   JOIN tracks t ON t.id = tw.track_id
                  WHERE t.release_id = ?1
                 UNION
                 SELECT wp.parent_work_id FROM work_parts wp
                   JOIN component c ON wp.child_work_id = c.work_id
                 UNION
                 SELECT wp.child_work_id FROM work_parts wp
                   JOIN component c ON wp.parent_work_id = c.work_id
             )
             SELECT work_id FROM component",
        params![release_id],
        |row| row.get(0),
    )?;

    // Composers link only through work_artists, so add them as candidates.
    for work_id in &candidate_work_ids {
        candidate_artist_ids.extend(sql.query(
            "SELECT artist_id FROM work_artists WHERE work_id = ?1",
            params![work_id],
            |row| row.get::<_, String>(0),
        )?);
    }

    // Works that survive once this release's track_works are gone — reachable from
    // a track on another release, directly or through the work_parts hierarchy.
    let live_work_ids: std::collections::HashSet<String> = sql
        .query(
            "WITH RECURSIVE live(work_id) AS (
                 SELECT DISTINCT work_id FROM track_works
                   WHERE track_id NOT IN (SELECT id FROM tracks WHERE release_id = ?1)
                 UNION
                 SELECT wp.parent_work_id FROM work_parts wp
                   JOIN live l ON wp.child_work_id = l.work_id
                 UNION
                 SELECT wp.child_work_id FROM work_parts wp
                   JOIN live l ON wp.parent_work_id = l.work_id
             )
             SELECT work_id FROM live",
            params![release_id],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .collect();
    let orphaned_work_ids: Vec<String> = candidate_work_ids
        .into_iter()
        .filter(|work_id| !live_work_ids.contains(work_id))
        .collect();

    let mut image_blobs: Vec<(&'static str, String, Option<String>)> = Vec::new();
    // The cover is 1:1 with the release, so it always orphans.
    if let Some((blob_id, cloud_path)) = sql
        .query_row(
            "SELECT blob_id, cloud_path FROM covers WHERE id = ?",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?
    {
        image_blobs.push((crate::sync::COVERS_NAMESPACE, blob_id, cloud_path));
    }

    // An artist orphans when nothing outside the deleted subtree names it. The
    // work_artists clause excludes the orphaned works (their rows cascade away);
    // an empty orphaned-work set drops the exclusion so every work_artists row
    // counts as surviving.
    let work_exclusion = if orphaned_work_ids.is_empty() {
        String::new()
    } else {
        let placeholders: Vec<String> = (0..orphaned_work_ids.len())
            .map(|i| format!("?{}", i + 5))
            .collect();
        format!("AND work_id NOT IN ({})", placeholders.join(", "))
    };
    let orphan_check = format!(
        "SELECT NOT (
             EXISTS(SELECT 1 FROM albums WHERE artist_id = ?1 AND NOT (id = ?2 AND ?3))
             OR EXISTS(SELECT 1 FROM album_artists WHERE artist_id = ?1 AND NOT (album_id = ?2 AND ?3))
             OR EXISTS(SELECT 1 FROM track_artists WHERE artist_id = ?1
                        AND track_id NOT IN (SELECT id FROM tracks WHERE release_id = ?4))
             OR EXISTS(SELECT 1 FROM work_artists WHERE artist_id = ?1 {work_exclusion})
             OR EXISTS(SELECT 1 FROM release_artist_roles WHERE artist_id = ?1 AND release_id != ?4)
             OR EXISTS(SELECT 1 FROM track_artist_roles WHERE artist_id = ?1
                        AND track_id NOT IN (SELECT id FROM tracks WHERE release_id = ?4))
         )"
    );

    let mut orphaned_artist_ids: Vec<String> = Vec::new();
    for artist_id in &candidate_artist_ids {
        let mut binds: Vec<&dyn coven::rusqlite::ToSql> =
            vec![artist_id, &album_id, &delete_album, &release_id];
        for work_id in &orphaned_work_ids {
            binds.push(work_id);
        }
        let orphaned: bool = sql.query_row(&orphan_check, binds.as_slice(), |row| row.get(0))?;
        if orphaned {
            orphaned_artist_ids.push(artist_id.clone());
            if let Some((blob_id, cloud_path)) = sql
                .query_row(
                    "SELECT blob_id, cloud_path FROM artist_images WHERE id = ?",
                    params![artist_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .optional()?
            {
                image_blobs.push((crate::sync::ARTIST_IMAGES_NAMESPACE, blob_id, cloud_path));
            }
        }
    }

    let file_ids: Vec<String> = sql.query(
        "SELECT id FROM release_files WHERE release_id = ?",
        params![release_id],
        |row| row.get(0),
    )?;

    Ok(FailImportDeletion {
        release_id: release_id.to_string(),
        album_id,
        delete_album,
        orphaned_work_ids,
        orphaned_artist_ids,
        image_blobs,
        file_ids,
    })
}
