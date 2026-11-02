//! Per-import answers to a label's conflicting catalog identities, and the
//! merge the "same label" answer makes. The label's statement stays as the
//! catalogs gave it; only this candidate's writes use the answer.

use super::*;
use crate::db::client::label_resolution::{AnsweredLabel, LabelAnswer, LabelRow};
use crate::import::LabelIdentityConflictChoice as Choice;
use bae_types::pressing::ReleaseLabel;

impl ImportTables {
    /// Record the person's answer to the candidate's label conflict, and
    /// dismiss the failure it stopped on. "Same label" merges the two library
    /// labels; changing the match moves the pane to its release picker and
    /// decides nothing.
    pub(crate) async fn choose_import_label_identity(
        &self,
        content_hash: &str,
        metadata_revision: u64,
        choice: Choice,
        ids: &dyn coven::IdSource,
    ) -> Result<(), DbError> {
        let hash = content_hash.to_string();
        let new_id = (choice == Choice::Neither).then(|| ids.new_id().to_string());
        let now = chrono::DateTime::<chrono::Utc>::from(self.database.now()).to_rfc3339();
        self.database
            .call(move |sql| {
                super::artist_identity_choice::require_revision(sql, &hash, metadata_revision)?;
                if choice == Choice::ChangeMatch {
                    // Going back does not decide the label or dismiss its failure.
                    return crate::import::tables::import_state::move_pane_on(
                        sql,
                        &hash,
                        crate::import::PaneMove::FindOnline,
                    );
                }
                let conflict = recorded_conflict_on(sql, &hash)?;
                let label_id = match choice {
                    Choice::SameLabel => {
                        merge_labels_on(sql, &conflict, &now)?;
                        conflict.discogs_library.clone()
                    }
                    Choice::DiscogsLabel => conflict.discogs_library.clone(),
                    Choice::MusicBrainzLabel => conflict.musicbrainz_library.clone(),
                    Choice::Neither => new_id.expect("Neither reserves a label id"),
                    Choice::ChangeMatch => unreachable!("changing the match returned above"),
                };
                record_choice_on(sql, &hash, &conflict, choice, &label_id)?;
                sql.execute(
                    "DELETE FROM import_candidate_failure WHERE content_hash = ?",
                    [&hash],
                )?;
                Ok(())
            })
            .await
    }
}

/// The candidate's recorded label conflict: the label's two catalog ids and
/// the library label holding each.
struct RecordedConflict {
    discogs: String,
    musicbrainz: String,
    discogs_library: String,
    musicbrainz_library: String,
}

fn recorded_conflict_on<Q: QueryOne>(sql: &Q, hash: &str) -> Result<RecordedConflict, DbError> {
    sql.query_row(
        "SELECT discogs_label_id, musicbrainz_label_id, discogs_library_label_id, \
                musicbrainz_library_label_id \
         FROM import_candidate_label_identity_conflict WHERE content_hash = ?",
        [hash],
        |row| {
            Ok(RecordedConflict {
                discogs: row.get(0)?,
                musicbrainz: row.get(1)?,
                discogs_library: row.get(2)?,
                musicbrainz_library: row.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| DbError::Message(format!("candidate {hash} has no label identity conflict")))
}

fn record_choice_on(
    sql: &CovenWrite<'_>,
    hash: &str,
    conflict: &RecordedConflict,
    choice: Choice,
    label_id: &str,
) -> Result<(), DbError> {
    let choice = match choice {
        Choice::SameLabel => "same_label",
        Choice::DiscogsLabel => "discogs_label",
        Choice::MusicBrainzLabel => "musicbrainz_label",
        Choice::Neither => "neither",
        Choice::ChangeMatch => {
            return Err(DbError::Message("changing the match does not choose a label".into()))
        }
    };
    sql.execute(
        "INSERT INTO import_candidate_label_identity_choice \
         (content_hash, discogs_label_id, musicbrainz_label_id, choice, label_id) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (content_hash, discogs_label_id, musicbrainz_label_id) DO UPDATE SET \
         choice = excluded.choice, label_id = excluded.label_id",
        params![hash, conflict.discogs, conflict.musicbrainz, choice, label_id],
    )?;
    sql.execute(
        "UPDATE import_candidate_state SET metadata_revision = metadata_revision + 1 \
         WHERE content_hash = ?",
        [hash],
    )?;
    Ok(())
}

/// Merge the conflict's MusicBrainz library label into its Discogs one: the
/// absorbed label is recorded as merged (`label_merges`), the survivor takes
/// its catalog ids, and every other candidate's conflict that the merge makes
/// one label is dismissed, the rest retargeted to the survivor. Neither row
/// is deleted and no release's labels are rewritten: both are shared with
/// devices that may be naming the absorbed label while apart, and every read
/// shows the survivor.
fn merge_labels_on(
    sql: &CovenWrite<'_>,
    conflict: &RecordedConflict,
    now: &str,
) -> Result<(), DbError> {
    let survivor = &conflict.discogs_library;
    let absorbed = &conflict.musicbrainz_library;
    if survivor == absorbed {
        return Err(DbError::Message(
            "a label identity conflict names one library label twice".into(),
        ));
    }
    let row =
        |id: &str| sql.query_row("SELECT * FROM labels WHERE id = ?", [id], LabelRow::from_row);
    let (surviving, absorbed_row) = (row(survivor)?, row(absorbed)?);
    for label in [&surviving, &absorbed_row] {
        if !crate::db::label_ids_are_compatible(
            &label.existing(),
            Some(&conflict.musicbrainz),
            Some(&conflict.discogs),
        ) {
            return Err(DbError::Message(format!(
                "library label {} no longer holds only the recorded catalog ids",
                label.id
            )));
        }
    }
    // Every other candidate's conflict the merge makes one label goes; the
    // rest that named the absorbed label name the survivor.
    sql.execute(
        "DELETE FROM import_candidate_failure WHERE content_hash IN (\
             SELECT content_hash FROM import_candidate_label_identity_conflict \
             WHERE (discogs_library_label_id = ?1 OR musicbrainz_library_label_id = ?1) \
               AND CASE WHEN discogs_library_label_id = ?1 THEN ?2 \
                        ELSE discogs_library_label_id END \
                   = CASE WHEN musicbrainz_library_label_id = ?1 THEN ?2 \
                          ELSE musicbrainz_library_label_id END)",
        params![absorbed, survivor],
    )?;
    for column in ["discogs_library_label_id", "musicbrainz_library_label_id"] {
        sql.execute(
            &format!(
                "UPDATE import_candidate_label_identity_conflict \
                 SET {column} = ?1 WHERE {column} = ?2"
            ),
            params![survivor, absorbed],
        )?;
    }
    sql.execute(
        "UPDATE labels SET \
             musicbrainz_label_id = COALESCE(musicbrainz_label_id, ?2), \
             discogs_label_id = COALESCE(discogs_label_id, ?3) \
         WHERE id = ?1",
        params![survivor, conflict.musicbrainz, conflict.discogs],
    )?;
    // A chain that ended at the absorbed label now ends at the survivor.
    sql.execute(
        "UPDATE label_merges SET into_label_id = ?1 WHERE into_label_id = ?2",
        params![survivor, absorbed],
    )?;
    sql.execute(
        "INSERT INTO label_merges (id, into_label_id, created_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (id) DO UPDATE SET into_label_id = excluded.into_label_id",
        params![absorbed, survivor, now],
    )?;
    Ok(())
}

/// The answers the candidate `hash` holds for `labels`, each carrying both
/// catalog ids: what an import of it writes its labels by.
pub(crate) fn label_answers_on<Q: QueryOne>(
    sql: &Q,
    hash: &str,
    labels: &[ReleaseLabel],
) -> Result<Vec<LabelAnswer>, DbError> {
    // Only a label carrying both ids can have stopped an import on a
    // conflict between them, so only such a label has an answer.
    let both_ids = labels
        .iter()
        .filter_map(|label| Some((label.discogs_label_id()?, label.musicbrainz_label_id()?)));
    let mut answers = Vec::new();
    for (discogs, musicbrainz) in both_ids {
        if let Some(answered) = chosen_label_on(sql, hash, discogs, musicbrainz)? {
            answers.push(LabelAnswer {
                discogs_label_id: discogs.to_string(),
                musicbrainz_label_id: musicbrainz.to_string(),
                label: answered,
            });
        }
    }
    Ok(answers)
}

/// The answer the candidate `hash` holds for a label carrying `discogs` and
/// `musicbrainz`, by the library label shown for it where it names one.
fn chosen_label_on<Q: QueryOne>(
    sql: &Q,
    hash: &str,
    discogs: &str,
    musicbrainz: &str,
) -> Result<Option<AnsweredLabel>, DbError> {
    let Some((choice, label_id)) = sql
        .query_row(
            "SELECT choice, label_id FROM import_candidate_label_identity_choice \
             WHERE content_hash = ? AND discogs_label_id = ? AND musicbrainz_label_id = ?",
            params![hash, discogs, musicbrainz],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let shown = shown_label_on(sql, &label_id)?;
    Ok(Some(match (choice.as_str(), shown) {
        ("same_label" | "discogs_label" | "musicbrainz_label" | "neither", Some(label)) => {
            AnsweredLabel::Library(label.id)
        }
        ("neither", None) => AnsweredLabel::New(label_id),
        ("same_label" | "discogs_label" | "musicbrainz_label", None) => {
            return Err(DbError::Message(format!(
                "the label chosen for candidate {hash}, {label_id}, is not in the library"
            )))
        }
        (other, _) => {
            return Err(DbError::Message(format!("unknown label identity choice {other:?}")))
        }
    }))
}

/// The label `label_id` shows as.
fn shown_label_on<Q: QueryOne>(sql: &Q, label_id: &str) -> Result<Option<LabelRow>, DbError> {
    sql.query_row(
        &format!(
            "SELECT * FROM labels WHERE id = {}",
            crate::db::client::label_resolution::shown_label_id("?1")
        ),
        [label_id],
        LabelRow::from_row,
    )
    .optional()
    .map_err(DbError::from)
}
