//! The library album a person picked for a candidate to join
//! (`import_candidate_album_pick`), over the one its records name.
//! Its table is `album_pick_rows.sql`, which every device's library holds.

use super::*;

// What the person picked for the album of each candidate they picked one for:
// the library album `picked_album_id`, or a new album when it is NULL.
sqlive_compose::fragment!(
    AlbumPickRow {},
    r#"
    SELECT pick.content_hash AS pick_hash, pick.album_id AS picked_album_id
    FROM import_candidate_album_pick AS pick
"#
);

/// The album the person picked, from the pick row a candidate holds: with no
/// row they picked none, and the candidate joins the album its records name.
pub(super) fn album_pick_of(row: Option<AlbumPickRow>) -> crate::AlbumPick {
    match row {
        None => crate::AlbumPick::Records,
        Some(AlbumPickRow { picked_album_id: None, .. }) => crate::AlbumPick::New,
        Some(AlbumPickRow { picked_album_id: Some(album_id), .. }) => {
            crate::AlbumPick::Album { album_id }
        }
    }
}

/// Leave the candidate `content_hash` to the album its records name.
pub(super) fn delete_album_pick(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_album_pick WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

impl ImportTables {
    /// Record `pick` for the candidate at `key`. Refused unless the write
    /// reads it as a release that may be worked on and edited. The table's
    /// foreign key refuses an album the library no longer holds.
    pub(crate) async fn set_candidate_album_pick(
        &self,
        key: &crate::CandidateKey,
        pick: crate::AlbumPick,
    ) -> Result<(), crate::ImportError> {
        let key = key.clone();
        self.store
            .write(move |sql| {
                let entry = crate::tables::release_groupings::require_workable_on(&sql, &key)?;
                entry.standing().editable()?;
                let content_hash = entry.candidate.files.content_hash();
                let album_id = match pick {
                    crate::AlbumPick::Records => {
                        return Ok::<_, crate::ImportError>(delete_album_pick(&sql, &content_hash)?)
                    }
                    crate::AlbumPick::New => None,
                    crate::AlbumPick::Album { album_id } => Some(album_id),
                };
                sql.execute(
                    "INSERT INTO import_candidate_album_pick (content_hash, album_id) \
                     VALUES (?, ?) \
                     ON CONFLICT (content_hash) DO UPDATE SET album_id = excluded.album_id",
                    params![content_hash, album_id],
                )?;
                Ok(())
            })
            .await
    }
}
