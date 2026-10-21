//! Per-import answers to conflicting catalog identities. The source credit
//! stays intact; only this candidate's writes and previews use the answer.

use super::artist_resolution::{ArtistCredits, ResolvedArtists};
use super::*;
use crate::import::ArtistIdentityConflictChoice as Choice;

impl Database {
    pub(crate) async fn choose_import_artist_identity(
        &self,
        content_hash: &str,
        metadata_revision: u64,
        choice: Choice,
        ids: &dyn coven::IdSource,
    ) -> Result<(), DbError> {
        if choice == Choice::SameArtist {
            return self
                .merge_import_artist_identity_choice(content_hash, metadata_revision)
                .await;
        }
        let hash = content_hash.to_string();
        let new_id = (choice == Choice::Neither).then(|| ids.new_id().to_string());
        self.call(move |sql| {
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
                    return super::import_state::move_pane_on(
                        sql,
                        &hash,
                        crate::import::PaneMove::FindOnline,
                    );
                }
                Choice::SameArtist => unreachable!("a merge uses the file-aware write"),
            };
            record_choice_on(sql, &hash, &discogs, &musicbrainz, choice, &artist_id)?;
            sql.execute(
                "DELETE FROM import_candidate_failure WHERE content_hash = ?",
                [&hash],
            )?;
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
    let revision = u64::try_from(revision).map_err(|error| {
        DbError::Message(format!("candidate metadata revision is negative: {error}"))
    })?;
    if revision != expected {
        return Err(DbError::Message(
            "the candidate changed before its artist choice was saved".into(),
        ));
    }
    Ok(())
}

pub(super) fn record_choice_on(
    sql: &SqlContext<'_, '_>,
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
            return Err(DbError::Message(
                "changing the match does not choose an artist".into(),
            ))
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
            return Err(DbError::Message(format!(
                "unknown artist identity choice {other:?}"
            )))
        }
    };
    Ok(Some(selected))
}

impl ArtistCredits<'_> {
    pub(super) fn resolve_for_import_on<Q: QueryOne + QueryRows>(
        &self,
        sql: &Q,
        content_hash: Option<&str>,
    ) -> Result<ResolvedArtists, ArtistWriteError> {
        let Some(hash) = content_hash else {
            return self.resolve_on(sql);
        };
        let mut ordinary = Vec::new();
        let mut selected = HashMap::new();
        let mut new_artists = HashMap::new();
        for credit in self.credits {
            let choice = if self.picked.contains(&credit.id) {
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
        let mut resolved = ArtistCredits {
            credits: &ordinary,
            picked: self.picked,
        }
        .resolve_on(sql)?;
        selected.extend(resolved.id_map(&ordinary));
        resolved.ids = self
            .credits
            .iter()
            .map(|credit| selected[&credit.id].clone())
            .collect();
        resolved.inserts.extend(new_artists.into_values());
        Ok(resolved)
    }
}

pub(super) fn resolve_import_credits_on<'a, Q: QueryOne + QueryRows>(
    sql: &Q,
    hash: &str,
    credits: impl IntoIterator<Item = &'a crate::import::ArtistCredit>,
) -> Result<Vec<crate::import::ResolvedCredit>, DbError> {
    super::artist_resolution::resolve_credits_with(credits, |credit| {
        Ok(
            match chosen_artist_on(
                sql,
                hash,
                credit.discogs_artist_id.as_deref(),
                credit.musicbrainz_artist_id.as_deref(),
            )? {
                Some(ChosenArtist::Library(artist)) => crate::import::CreditResolution::Library {
                    artist: artist.into(),
                },
                Some(ChosenArtist::New(_)) => crate::import::CreditResolution::New,
                None => super::artist_resolution::resolve_credit_on(sql, credit)?,
            },
        )
    })
}
