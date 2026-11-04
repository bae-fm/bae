//! Per-import answers to conflicting catalog identities. The source credit
//! stays intact; only this candidate's writes and previews use the answer.

use self::ArtistIdentityConflictChoice as Choice;
use super::*;
use bae_db::client::artist_resolution::{resolve_credit_on, resolve_credits_with, ResolvedArtists};

/// The person's answer to an import's conflicting artist identities. Nothing
/// selects an answer automatically. The first four answers decide this credit;
/// changing the match opens the candidate's existing release picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistIdentityConflictChoice {
    /// Merge into the Discogs library artist, retaining both provider ids.
    SameArtist,
    /// Credit the Discogs library artist without attaching the incoming MusicBrainz id.
    DiscogsArtist,
    /// Credit the MusicBrainz library artist without attaching the Discogs id.
    MusicBrainzArtist,
    /// Create an independent artist without either provider id when importing.
    Neither,
    ChangeMatch,
}

impl ImportTables {
    pub async fn choose_import_artist_identity(
        &self,
        content_hash: &str,
        metadata_revision: u64,
        choice: Choice,
        ids: &dyn coven::IdSource,
    ) -> Result<(), DbError> {
        if choice == Choice::SameArtist {
            return self.merge_import_artist_identity_choice(content_hash, metadata_revision).await;
        }
        let hash = content_hash.to_string();
        let new_id = (choice == Choice::Neither).then(|| ids.new_id().to_string());
        self.database
            .call(move |sql| {
                require_revision(sql, &hash, metadata_revision)?;
                let (discogs, musicbrainz, discogs_library, musicbrainz_library): (
                    String,
                    String,
                    String,
                    String,
                ) = sql.query_row(
                    "SELECT discogs_artist_id, musicbrainz_artist_id, discogs_library_artist_id, \
                 musicbrainz_library_artist_id FROM import_candidate_artist_identity_conflict \
                 WHERE content_hash = ?",
                    [&hash],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
                let artist_id = match choice {
                    Choice::DiscogsArtist => discogs_library,
                    Choice::MusicBrainzArtist => musicbrainz_library,
                    Choice::Neither => new_id.expect("Neither reserves an artist id"),
                    Choice::ChangeMatch => {
                        // Going back does not decide the credit or dismiss its failure.
                        return crate::tables::import_state::move_pane_on(
                            sql,
                            &hash,
                            crate::PaneMove::FindOnline,
                        );
                    }
                    Choice::SameArtist => unreachable!("a merge uses the file-aware write"),
                };
                record_choice_on(sql, &hash, &discogs, &musicbrainz, choice, &artist_id)?;
                crate::tables::import_state::failure_rows::delete_failure_on(sql, &hash)?;
                Ok(())
            })
            .await
    }
}

pub(super) fn require_revision<Q: QueryOne>(
    sql: &Q,
    hash: &str,
    expected: u64,
) -> Result<(), DbError> {
    let revision: i64 = sql.query_row(
        "SELECT metadata_revision FROM import_candidate_state WHERE content_hash = ?",
        [hash],
        |row| row.get(0),
    )?;
    let revision = to_u64(revision, "import_candidate_state.metadata_revision")?;
    if revision != expected {
        return Err(DbError::Message(
            "the candidate changed before its artist choice was saved".into(),
        ));
    }
    Ok(())
}

/// Forget every artist identity answer given for `content_hash`.
pub(super) fn delete_choices_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_artist_identity_choice WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

pub(super) fn record_choice_on(
    sql: &CovenWrite<'_>,
    hash: &str,
    discogs: &str,
    musicbrainz: &str,
    choice: Choice,
    artist_id: &str,
) -> Result<(), DbError> {
    let choice = match choice {
        Choice::SameArtist => "same_artist",
        Choice::DiscogsArtist => "discogs_artist",
        Choice::MusicBrainzArtist => "musicbrainz_artist",
        Choice::Neither => "neither",
        Choice::ChangeMatch => {
            return Err(DbError::Message("changing the match does not choose an artist".into()))
        }
    };
    sql.execute(
        "INSERT INTO import_candidate_artist_identity_choice \
         (content_hash, discogs_artist_id, musicbrainz_artist_id, choice, artist_id) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (content_hash, discogs_artist_id, musicbrainz_artist_id) DO UPDATE SET \
         choice = excluded.choice, artist_id = excluded.artist_id",
        params![hash, discogs, musicbrainz, choice, artist_id],
    )?;
    sql.execute(
        "UPDATE import_candidate_state SET metadata_revision = metadata_revision + 1 WHERE content_hash = ?",
        [hash],
    )?;
    Ok(())
}

/// An explicit library selection, or the independent identity reserved for
/// Neither. The latter bypasses both catalog and name matching.
enum ChosenArtist {
    Library(DbArtist),
    New(String),
}

fn chosen_artist_on<Q: QueryOne>(
    sql: &Q,
    hash: &str,
    discogs: Option<&str>,
    musicbrainz: Option<&str>,
) -> Result<Option<ChosenArtist>, DbError> {
    let Some((choice, artist_id)) = sql
        .query_row(
            "SELECT choice, artist_id FROM import_candidate_artist_identity_choice \
         WHERE content_hash = ? AND discogs_artist_id = ? AND musicbrainz_artist_id = ?",
            params![hash, discogs, musicbrainz],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let selected = match choice.as_str() {
        "same_artist" | "discogs_artist" | "musicbrainz_artist" => {
            let artist = sql.query_row(
                &format!("SELECT * FROM artists WHERE id = {}", shown_artist_id("?1")),
                [&artist_id],
                row_to_artist,
            )?;
            ChosenArtist::Library(artist)
        }
        "neither" => match sql
            .query_row(
                &format!("SELECT * FROM artists WHERE id = {}", shown_artist_id("?1")),
                [&artist_id],
                row_to_artist,
            )
            .optional()?
        {
            Some(artist) => ChosenArtist::Library(artist),
            None => ChosenArtist::New(artist_id),
        },
        other => {
            return Err(DbError::unreadable(
                "import_candidate_artist_identity_choice.choice",
                other,
            ))
        }
    };
    Ok(Some(selected))
}

/// Resolve `credits` as an import of the candidate `content_hash` commits
/// them: a credit the person answered a conflict for is the artist they
/// chose, every other one is resolved by the artist rule.
pub(crate) fn resolve_for_import_on<Q: QueryOne + QueryRows>(
    credits: &ArtistCredits<'_>,
    sql: &Q,
    content_hash: Option<&str>,
) -> Result<ResolvedArtists, IdentityWriteError> {
    let Some(hash) = content_hash else {
        return credits.resolve_on(sql);
    };
    let mut ordinary = Vec::new();
    let mut selected = HashMap::new();
    let mut new_artists = HashMap::new();
    for credit in credits.credits {
        let choice = if credits.picked.contains(&credit.id) {
            None
        } else {
            chosen_artist_on(
                sql,
                hash,
                credit.discogs_artist_id.as_deref(),
                credit.musicbrainz_artist_id.as_deref(),
            )?
        };
        match choice {
            None => ordinary.push(credit.clone()),
            Some(ChosenArtist::Library(artist)) => {
                selected.insert(credit.id.clone(), artist.id);
            }
            Some(ChosenArtist::New(id)) => {
                selected.insert(credit.id.clone(), id.clone());
                new_artists.entry(id.clone()).or_insert_with(|| DbArtist {
                    id,
                    name: credit.name.clone(),
                    sort_name: credit.sort_name.clone(),
                    discogs_artist_id: None,
                    musicbrainz_artist_id: None,
                    created_at: credit.created_at,
                });
            }
        }
    }
    let mut resolved =
        ArtistCredits { credits: &ordinary, picked: credits.picked }.resolve_on(sql)?;
    selected.extend(resolved.id_map(&ordinary));
    resolved.ids = credits.credits.iter().map(|credit| selected[&credit.id].clone()).collect();
    resolved.inserts.extend(new_artists.into_values());
    Ok(resolved)
}

pub(crate) fn resolve_import_credits_on<'a, Q: QueryOne + QueryRows>(
    sql: &Q,
    hash: &str,
    credits: impl IntoIterator<Item = &'a bae_catalogs::credits::ArtistCredit>,
) -> Result<Vec<bae_db::ResolvedCredit>, DbError> {
    resolve_credits_with(credits, |credit| {
        Ok(
            match chosen_artist_on(
                sql,
                hash,
                credit.discogs_artist_id.as_deref(),
                credit.musicbrainz_artist_id.as_deref(),
            )? {
                Some(ChosenArtist::Library(artist)) => {
                    bae_db::CreditResolution::Library { artist: artist.into() }
                }
                Some(ChosenArtist::New(_)) => bae_db::CreditResolution::New,
                None => resolve_credit_on(sql, credit)?,
            },
        )
    })
}

impl ImportTables {
    /// Resolve `credits` against the library as it stands, without writing:
    /// what an import of the candidate `content_hash` committing them now
    /// would link and create.
    pub(crate) async fn resolve_artists(
        &self,
        credits: &[DbArtist],
        picked: &[String],
        content_hash: Option<&str>,
    ) -> Result<ResolvedArtists, IdentityWriteError> {
        let (credits, picked) = (credits.to_vec(), picked.to_vec());
        let content_hash = content_hash.map(str::to_string);
        self.database
            .read_resolving_identities(move |sql| {
                resolve_for_import_on(
                    &ArtistCredits { credits: &credits, picked: &picked },
                    &sql,
                    content_hash.as_deref(),
                )
            })
            .await
    }
}

#[cfg(test)]
#[path = "artist_identity_choice_tests.rs"]
mod tests;
