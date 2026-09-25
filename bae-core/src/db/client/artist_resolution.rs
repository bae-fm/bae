//! Which library artist an artist credit names — the one rule every write of
//! an artist credit decides by: an import committing its release, and a
//! metadata edit saving its artists. Each runs it inside the transaction that
//! writes the links, so it decides against the library as committed at that
//! moment and nothing another write lands in between can make it wrong.
//!
//! The rule, in order:
//!
//! 1. **A catalog id is decisive.** A credit carrying a MusicBrainz or Discogs
//!    id is the library artist holding that id, followed through any merge. Two
//!    different artists holding the credit's two ids is the conflict a person
//!    settles by merging them; a matched artist holding a different id of the
//!    same catalog is refused outright.
//! 2. **Otherwise the name.** A credit is the one library artist whose
//!    `name_key` equals the credit's folded name and whose catalog ids do not
//!    contradict the credit's. Several such artists leave it ambiguous, which
//!    commits as a new artist; none commits as a new artist.
//!
//! A credit linked to an existing artist fills in that artist's missing catalog
//! ids and sort name. The artist keeps its own id: row ids are never rewritten,
//! so an artist first created from a name alone keeps the independent id it was
//! created with after a later credit gives it a catalog id. Only a *new*
//! artist a catalog names takes the id [`crate::db::identity::artist_id`]
//! derives from that catalog entry.

use super::*;

/// Why a write that decides artist credits did not commit. Nothing of the
/// write landed; the initiator sees this and may retry once the library or
/// the credit changes.
#[derive(Debug, thiserror::Error)]
pub enum ArtistWriteError {
    /// Two library artists each hold one of the credit's catalog ids — the
    /// conflict a person resolves by merging them.
    #[error(transparent)]
    IdentityConflict(Box<crate::import::ArtistIdentityConflict>),
    /// The credit cannot be linked to the library as it stands: its ids
    /// contradict the artist they match, or a picked artist no longer exists.
    #[error("{0}")]
    Unresolvable(String),
    /// The library's artists changed between preparing the write and
    /// committing it in a way that makes what was prepared for them wrong:
    /// the import staged pictures for artists that are no longer new, or not
    /// for ones that now are. Nothing was written; importing again prepares
    /// against the library as it now stands.
    #[error("the library's artists changed while this import was being written; import it again")]
    LibraryChanged,
    #[error(transparent)]
    Db(#[from] DbError),
}

/// Carry an [`ArtistWriteError`] out of a coven write closure with its type
/// intact, so [`Database::artist_write_error`] can hand it back.
pub(super) fn artist_write_failure(error: ArtistWriteError) -> CovenError {
    match error {
        ArtistWriteError::Db(error) => CovenError::from(error),
        other => CovenError::Host(Box::new(other)),
    }
}

impl Database {
    /// A write that resolves artist credits: [`Self::call_sql`], with an
    /// [`ArtistWriteError`] the closure fails with handed back as itself.
    pub(super) async fn write_resolving_artists<R>(
        &self,
        f: impl for<'ctx, 'conn> FnOnce(SqlContext<'ctx, 'conn>) -> Result<R, ArtistWriteError>
            + Send
            + 'static,
    ) -> Result<R, ArtistWriteError>
    where
        R: Send + 'static,
    {
        self.inner
            .handle
            .write(move |sql| f(sql).map_err(artist_write_failure))
            .await
            .map(|receipt| receipt.value)
            .map_err(Self::artist_write_error)
    }

    /// Resolve `credits` against the library as it stands, without writing:
    /// what a write committing them now would link and create. Only an
    /// import previews its resolution, and the mobile builds do not import.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn resolve_artists(
        &self,
        credits: &[DbArtist],
        picked: &[String],
    ) -> Result<ResolvedArtists, ArtistWriteError> {
        let (credits, picked) = (credits.to_vec(), picked.to_vec());
        self.inner
            .handle
            .read(move |sql| {
                ArtistCredits {
                    credits: &credits,
                    picked: &picked,
                }
                .resolve_on(&sql)
                .map_err(artist_write_failure)
            })
            .await
            .map_err(Self::artist_write_error)
    }

    /// Resolve `credits` and write the rows the resolution inserts and fills
    /// in, in one transaction: the artist step of an import or edit, without
    /// the release around it.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn find_or_create_artists(
        &self,
        credits: &[DbArtist],
    ) -> Result<Vec<String>, ArtistWriteError> {
        let credits = credits.to_vec();
        self.write_resolving_artists(move |sql| {
            let reg = sql.stamp();
            let resolved = ArtistCredits {
                credits: &credits,
                picked: &[],
            }
            .resolve_on(&sql)?;
            resolved.write_on(&sql, &reg)?;
            Ok(resolved.ids)
        })
        .await
    }

    /// What the library holds for each of `credits` as it stands now.
    pub async fn resolve_artist_credits(
        &self,
        credits: &[crate::import::ArtistCredit],
    ) -> Result<Vec<crate::import::ResolvedCredit>, DbError> {
        let credits = credits.to_vec();
        self.read(move |sql| resolve_credits_on(&sql, &credits))
            .await
    }

    /// The [`ArtistWriteError`] a coven write closure failed with, or the
    /// database error it failed with otherwise.
    pub(super) fn artist_write_error(error: CovenError) -> ArtistWriteError {
        match error {
            CovenError::Host(host) => match host.downcast::<ArtistWriteError>() {
                Ok(error) => *error,
                Err(other) => ArtistWriteError::Db(Self::coven_error(CovenError::Host(other))),
            },
            other => ArtistWriteError::Db(Self::coven_error(other)),
        }
    }
}

impl From<coven::rusqlite::Error> for ArtistWriteError {
    fn from(error: coven::rusqlite::Error) -> Self {
        Self::Db(error.into())
    }
}

/// The resolved artist of every credit an import or edit writes, in input
/// order, with the rows the same write inserts or fills in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedArtists {
    /// The library id each input credit resolved to, positionally.
    pub ids: Vec<String>,
    /// New artists to insert, each with its final id.
    pub inserts: Vec<DbArtist>,
    /// `(artist_id, artist)` — an existing artist whose empty catalog ids and
    /// sort name this write fills in.
    pub external_id_updates: Vec<(String, DbArtist)>,
}

impl ResolvedArtists {
    /// Insert the new artists and fill in the linked ones, before any link
    /// that points at them is written.
    pub(super) fn write_on(&self, tx: &SqlContext<'_, '_>, reg: &str) -> Result<(), DbError> {
        for artist in &self.inserts {
            insert_artist_row(tx, artist, reg)?;
        }
        for (artist_id, artist) in &self.external_id_updates {
            update_artist_external_ids_row(
                tx,
                artist_id,
                artist.discogs_artist_id.as_deref(),
                artist.musicbrainz_artist_id.as_deref(),
                artist.sort_name.as_deref(),
                reg,
            )?;
        }
        Ok(())
    }

    /// The resolved id of every input credit, keyed by the id it came in with.
    pub(super) fn id_map(&self, credits: &[DbArtist]) -> HashMap<String, String> {
        credits
            .iter()
            .zip(&self.ids)
            .map(|(credit, id)| (credit.id.clone(), id.clone()))
            .collect()
    }
}

/// The artist credits one write links, as its links name them. A credit whose
/// id is in `picked` is a library artist a person chose; every other credit is
/// what a source or a person said, resolved by [`resolve_artists_on`] when the
/// write commits.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ArtistCredits<'a> {
    pub credits: &'a [DbArtist],
    pub picked: &'a [String],
}

impl ArtistCredits<'_> {
    pub(super) fn resolve_on<Q: QueryOne + QueryRows>(
        &self,
        sql: &Q,
    ) -> Result<ResolvedArtists, ArtistWriteError> {
        let picked: HashSet<String> = self.picked.iter().cloned().collect();
        resolve_artists_on(sql, self.credits, &picked)
    }
}

/// Resolve every credit an import or edit writes. `picked` names the credits
/// that are library artists a person chose: each resolves to itself, and must
/// still exist. Every other credit goes through the rule in the module doc,
/// against the committed library and against the rows this same write already
/// decided — so two credits of one release naming one new artist insert it
/// once, and a credit meeting an artist another credit of this write just
/// filled in sees the filled-in ids.
///
/// Within one write a credit first meets the new artists this write already
/// decided on, then the library, so two credits that are each ambiguous
/// against the library still become one new artist.
pub(crate) fn resolve_artists_on<Q: QueryOne + QueryRows>(
    sql: &Q,
    credits: &[DbArtist],
    picked: &HashSet<String>,
) -> Result<ResolvedArtists, ArtistWriteError> {
    let mut ids = Vec::with_capacity(credits.len());
    let mut inserts: Vec<DbArtist> = Vec::new();
    let mut external_id_updates: Vec<(String, DbArtist)> = Vec::new();

    for credit in credits {
        if picked.contains(&credit.id) {
            if artist_by_id_on(sql, &credit.id)?.is_none() {
                return Err(ArtistWriteError::Unresolvable(format!(
                    "artist '{}' no longer exists",
                    credit.id
                )));
            }
            ids.push(credit.id.clone());
            continue;
        }

        let committed = match catalog_match_on(sql, credit, &external_id_updates)? {
            CatalogMatch::Unmatched => None,
            CatalogMatch::Artist(artist) => Some(artist),
            CatalogMatch::Conflict(conflict) => return Err(conflict.into_error()),
        };
        let pending_by_catalog = pending_catalog_indices(&inserts, credit);
        let resolved_id = match committed {
            Some(existing) => {
                let id = existing.id.clone();
                let mut merged = existing;
                let absorbed = pending_by_catalog
                    .iter()
                    .map(|index| inserts[*index].clone())
                    .collect::<Vec<_>>();
                for pending in &absorbed {
                    merge_artist_metadata(&mut merged, pending)?;
                }
                merge_artist_metadata(&mut merged, credit)?;
                remove_pending_artists(&mut inserts, &mut ids, &absorbed, &id);
                stage_artist_update(&mut external_id_updates, id.clone(), merged);
                id
            }
            None => match pending_by_catalog.first().copied() {
                Some(survivor) => {
                    let survivor_id = inserts[survivor].id.clone();
                    let absorbed = pending_by_catalog
                        .iter()
                        .skip(1)
                        .map(|index| inserts[*index].clone())
                        .collect::<Vec<_>>();
                    for pending in &absorbed {
                        merge_artist_metadata(&mut inserts[survivor], pending)?;
                    }
                    merge_artist_metadata(&mut inserts[survivor], credit)?;
                    remove_pending_artists(&mut inserts, &mut ids, &absorbed, &survivor_id);
                    survivor_id
                }
                None => resolve_by_name_on(sql, credit, &mut inserts, &mut external_id_updates)?,
            },
        };
        ids.push(resolved_id);
    }

    // A new artist a catalog names gets the id that catalog entry names, so
    // every device that meets this artist writes one row.
    for insert in &mut inserts {
        let Some(identity) = crate::db::identity::artist_id(
            insert.musicbrainz_artist_id.as_deref(),
            insert.discogs_artist_id.as_deref(),
        ) else {
            continue;
        };
        for id in ids.iter_mut().filter(|id| **id == insert.id) {
            *id = identity.clone();
        }
        insert.id = identity;
    }

    Ok(ResolvedArtists {
        ids,
        inserts,
        external_id_updates,
    })
}

/// What the library holds for one credit as it stands now: the rule every
/// write commits by, applied to the credit alone. What the import pane shows
/// beside a credit, read inside its live query so an artist another import
/// commits reads it again.
pub(crate) fn resolve_credit_on<Q: QueryOne + QueryRows>(
    sql: &Q,
    credit: &crate::import::ArtistCredit,
) -> Result<crate::import::CreditResolution, DbError> {
    use crate::import::CreditResolution;
    let row = DbArtist {
        id: String::new(),
        name: credit.name.clone(),
        sort_name: credit.sort_name.clone(),
        discogs_artist_id: credit.discogs_artist_id.clone(),
        musicbrainz_artist_id: credit.musicbrainz_artist_id.clone(),
        created_at: chrono::DateTime::<Utc>::MIN_UTC,
    };
    Ok(match catalog_match_on(sql, &row, &[])? {
        CatalogMatch::Artist(artist) => CreditResolution::Library {
            artist: artist.into(),
        },
        CatalogMatch::Conflict(conflict) => CreditResolution::Conflicting {
            artists: conflict.artists(),
        },
        CatalogMatch::Unmatched => {
            let mut named = name_matches_on(sql, &row, &[])?;
            match named.len() {
                0 => CreditResolution::New,
                1 => CreditResolution::Library {
                    artist: named.remove(0).into(),
                },
                _ => CreditResolution::Ambiguous {
                    artists: named.into_iter().map(Into::into).collect(),
                },
            }
        }
    })
}

/// Every distinct credit of `assignments`, resolved as [`resolve_credit_on`]
/// resolves it, in the order the credits first appear.
pub(crate) fn resolve_credits_on<'a, Q: QueryOne + QueryRows>(
    sql: &Q,
    credits: impl IntoIterator<Item = &'a crate::import::ArtistCredit>,
) -> Result<Vec<crate::import::ResolvedCredit>, DbError> {
    let mut resolved: Vec<crate::import::ResolvedCredit> = Vec::new();
    for credit in credits {
        if resolved.iter().any(|known| known.credit == *credit) {
            continue;
        }
        resolved.push(crate::import::ResolvedCredit {
            credit: credit.clone(),
            resolution: resolve_credit_on(sql, credit)?,
        });
    }
    Ok(resolved)
}

/// The name step for a credit no catalog id placed: the one new artist of
/// this write it names, else the one library artist, else a new artist.
fn resolve_by_name_on<Q: QueryOne + QueryRows>(
    sql: &Q,
    credit: &DbArtist,
    inserts: &mut Vec<DbArtist>,
    external_id_updates: &mut Vec<(String, DbArtist)>,
) -> Result<String, ArtistWriteError> {
    let key = crate::util::text::normalize(&credit.name);
    let pending: Vec<usize> = inserts
        .iter()
        .enumerate()
        .filter(|(_, pending)| {
            !key.is_empty()
                && crate::util::text::normalize(&pending.name) == key
                && ids_agree(pending, credit)
        })
        .map(|(index, _)| index)
        .collect();
    if let [only] = pending.as_slice() {
        merge_artist_metadata(&mut inserts[*only], credit)?;
        return Ok(inserts[*only].id.clone());
    }
    if pending.is_empty() {
        if let [only] = name_matches_on(sql, credit, external_id_updates)?.as_slice() {
            let mut merged = only.clone();
            merge_artist_metadata(&mut merged, credit)?;
            let id = merged.id.clone();
            stage_artist_update(external_id_updates, id.clone(), merged);
            return Ok(id);
        }
    }
    inserts.push(credit.clone());
    Ok(credit.id.clone())
}

/// `links` with each one's artist rewritten from the credit id it names to
/// the library artist that credit resolved to.
pub(super) fn relink<T: Clone>(
    links: &[T],
    artist_ids: &HashMap<String, String>,
    label: &str,
    artist_id: impl Fn(&mut T) -> &mut String,
) -> Result<Vec<T>, DbError> {
    links
        .iter()
        .map(|link| {
            let mut link = link.clone();
            let credit_id = artist_id(&mut link);
            let resolved = artist_ids.get(credit_id.as_str()).ok_or_else(|| {
                DbError::Message(format!(
                    "{label} names {credit_id}, which no credit of the write is"
                ))
            })?;
            credit_id.clone_from(resolved);
            Ok(link)
        })
        .collect()
}

/// The album's primary artist, resolved.
pub(super) fn relinked_album(
    album: &DbAlbum,
    artist_ids: &HashMap<String, String>,
) -> Result<DbAlbum, DbError> {
    let mut album = relink(std::slice::from_ref(album), artist_ids, "album", |album| {
        &mut album.artist_id
    })?;
    Ok(album.remove(0))
}

/// The album's further artists, resolved. Two credits that resolve to one
/// artist are one credit: a later one naming the primary artist, or an artist
/// an earlier position already holds, is dropped.
pub(super) fn relink_album_artists(
    album: Option<&DbAlbum>,
    links: &[DbAlbumArtist],
    artist_ids: &HashMap<String, String>,
) -> Result<Vec<DbAlbumArtist>, DbError> {
    let mut seen: HashSet<String> = album
        .map(|album| relinked_album(album, artist_ids))
        .transpose()?
        .map(|album| album.artist_id)
        .into_iter()
        .collect();
    let mut links = relink(links, artist_ids, "album artist", |link| {
        &mut link.artist_id
    })?;
    links.sort_by_key(|link| link.position);
    links.retain(|link| seen.insert(link.artist_id.clone()));
    Ok(links)
}

/// Each track's artists, resolved, with a later credit dropped where it
/// resolves to an artist the track already holds.
pub(super) fn relink_track_artists(
    links: &[DbTrackArtist],
    artist_ids: &HashMap<String, String>,
) -> Result<Vec<DbTrackArtist>, DbError> {
    let mut links = relink(links, artist_ids, "track artist", |link| {
        &mut link.artist_id
    })?;
    links.sort_by(|a, b| (&a.track_id, a.position).cmp(&(&b.track_id, b.position)));
    let mut seen: HashSet<(String, String)> = HashSet::new();
    links.retain(|link| seen.insert((link.track_id.clone(), link.artist_id.clone())));
    Ok(links)
}

/// What the catalog-id step says about one credit.
pub(super) enum CatalogMatch {
    /// The credit carries no id the library holds.
    Unmatched,
    /// The library artist its ids name.
    Artist(DbArtist),
    /// Its ids name library artists that disagree with it.
    Conflict(CatalogConflict),
}

pub(super) enum CatalogConflict {
    /// Two different artists each hold one of the credit's ids.
    Identity(Box<crate::import::ArtistIdentityConflict>),
    /// A matched artist holds a different id of the same catalog, or the
    /// library holds one id on several artists.
    Contradicted {
        artists: Vec<DbArtist>,
        detail: String,
    },
}

impl CatalogConflict {
    fn into_error(self) -> ArtistWriteError {
        match self {
            Self::Identity(conflict) => ArtistWriteError::IdentityConflict(conflict),
            Self::Contradicted { detail, .. } => ArtistWriteError::Unresolvable(detail),
        }
    }

    /// The library artists the credit's ids point at.
    fn artists(self) -> Vec<crate::import::ExistingArtist> {
        match self {
            Self::Identity(conflict) => {
                let crate::import::ArtistIdentityConflict {
                    discogs_artist,
                    musicbrainz_artist,
                    ..
                } = *conflict;
                vec![discogs_artist, musicbrainz_artist]
            }
            Self::Contradicted { artists, .. } => artists.into_iter().map(Into::into).collect(),
        }
    }
}

/// The catalog-id step: the library artist the credit's Discogs and
/// MusicBrainz ids name, as `staged` (this write's own fills) leaves it.
pub(super) fn catalog_match_on<Q: QueryOne>(
    sql: &Q,
    credit: &DbArtist,
    staged: &[(String, DbArtist)],
) -> Result<CatalogMatch, DbError> {
    let by_discogs = match credit.discogs_artist_id.as_deref() {
        Some(id) => match exact_artist(
            "Discogs",
            &credit.name,
            id,
            artist_by_catalog_id_on(sql, CatalogColumn::Discogs, id)?,
            staged,
            |artist| artist.discogs_artist_id.as_deref(),
        ) {
            Ok(artist) => artist,
            Err(conflict) => return Ok(CatalogMatch::Conflict(conflict)),
        },
        None => None,
    };
    let by_musicbrainz = match credit.musicbrainz_artist_id.as_deref() {
        Some(id) => match exact_artist(
            "MusicBrainz",
            &credit.name,
            id,
            artist_by_catalog_id_on(sql, CatalogColumn::MusicBrainz, id)?,
            staged,
            |artist| artist.musicbrainz_artist_id.as_deref(),
        ) {
            Ok(artist) => artist,
            Err(conflict) => return Ok(CatalogMatch::Conflict(conflict)),
        },
        None => None,
    };
    let matched = match matching_artist(credit, by_discogs, by_musicbrainz) {
        Ok(matched) => matched,
        Err(conflict) => return Ok(CatalogMatch::Conflict(conflict)),
    };
    if let Some(artist) = matched {
        return Ok(CatalogMatch::Artist(artist));
    }
    if !credit.is_various_artists() {
        return Ok(CatalogMatch::Unmatched);
    }

    // The two provider IDs are one canonical cross-provider identity. An
    // incoming exact provider match still wins; only its absence permits the
    // other provider's canonical row to stand in for it.
    let va = &crate::db::VARIOUS_ARTISTS;
    let stand_in = match (
        credit.discogs_artist_id.as_deref(),
        credit.musicbrainz_artist_id.as_deref(),
    ) {
        (Some(_), None) => exact_artist(
            "MusicBrainz",
            &credit.name,
            va.musicbrainz,
            artist_by_catalog_id_on(sql, CatalogColumn::MusicBrainz, va.musicbrainz)?,
            staged,
            |artist| artist.musicbrainz_artist_id.as_deref(),
        ),
        (None, Some(_)) => exact_artist(
            "Discogs",
            &credit.name,
            va.discogs,
            artist_by_catalog_id_on(sql, CatalogColumn::Discogs, va.discogs)?,
            staged,
            |artist| artist.discogs_artist_id.as_deref(),
        ),
        (Some(_), Some(_)) | (None, None) => Ok(None),
    };
    Ok(match stand_in {
        Ok(Some(artist)) => CatalogMatch::Artist(artist),
        Ok(None) => CatalogMatch::Unmatched,
        Err(conflict) => CatalogMatch::Conflict(conflict),
    })
}

/// Every shown library artist whose folded name is the credit's and whose
/// catalog ids — as this write's own fills leave them — agree with the
/// credit's. Absorbed artists are left out: a merge shows them as their
/// survivor, which is matched on its own name.
pub(super) fn name_matches_on<Q: QueryRows>(
    sql: &Q,
    credit: &DbArtist,
    staged: &[(String, DbArtist)],
) -> Result<Vec<DbArtist>, DbError> {
    let key = crate::util::text::normalize(&credit.name);
    if key.is_empty() {
        return Ok(Vec::new());
    }
    let named = sql.query(
        &format!(
            "SELECT * FROM artists WHERE name_key = ?1 AND {shown} ORDER BY id",
            shown = artist_is_shown("artists")
        ),
        params![key],
        row_to_artist,
    )?;
    Ok(named
        .into_iter()
        .map(|artist| {
            staged
                .iter()
                .find(|(id, _)| *id == artist.id)
                .map(|(_, staged)| staged.clone())
                .unwrap_or(artist)
        })
        .filter(|artist| ids_agree(artist, credit))
        .collect())
}

#[derive(Clone, Copy)]
enum CatalogColumn {
    Discogs,
    MusicBrainz,
}

/// The artist whose `column` holds `catalog_id`, or whose row the catalog id
/// names (its column may have been filled in or edited since), followed
/// through any merge to the artist it shows as.
fn artist_by_catalog_id_on<Q: QueryOne>(
    sql: &Q,
    column: CatalogColumn,
    catalog_id: &str,
) -> Result<Option<DbArtist>, DbError> {
    let (column, identity) = match column {
        CatalogColumn::Discogs => (
            "discogs_artist_id",
            crate::db::identity::artist_id(None, Some(catalog_id)),
        ),
        CatalogColumn::MusicBrainz => (
            "musicbrainz_artist_id",
            crate::db::identity::artist_id(Some(catalog_id), None),
        ),
    };
    sql.query_row(
        &format!(
            "SELECT a.* FROM artists a WHERE a.id IN ( \
                 SELECT {} FROM artists named \
                 WHERE named.{column} = ?1 OR named.id = ?2) \
             ORDER BY a.id = ?2 DESC, a.id LIMIT 1",
            shown_artist_id("named.id")
        ),
        params![catalog_id, identity],
        row_to_artist,
    )
    .optional()
    .map_err(DbError::from)
}

fn artist_by_id_on<Q: QueryOne>(sql: &Q, artist_id: &str) -> Result<Option<DbArtist>, DbError> {
    sql.query_row(
        "SELECT * FROM artists WHERE id = ?",
        params![artist_id],
        row_to_artist,
    )
    .optional()
    .map_err(DbError::from)
}

fn exact_artist(
    source: &str,
    incoming_name: &str,
    source_id: &str,
    stored: Option<DbArtist>,
    staged_updates: &[(String, DbArtist)],
    external_id: impl for<'a> Fn(&'a DbArtist) -> Option<&'a str>,
) -> Result<Option<DbArtist>, CatalogConflict> {
    let staged_matches: Vec<&DbArtist> = staged_updates
        .iter()
        .map(|(_, artist)| artist)
        .filter(|artist| external_id(artist) == Some(source_id))
        .collect();
    if staged_matches.len() > 1 {
        return Err(CatalogConflict::Contradicted {
            artists: staged_matches.into_iter().cloned().collect(),
            detail: format!(
                "artist '{incoming_name}' has a {source} source ID staged for multiple library artists"
            ),
        });
    }
    let staged = staged_matches.first().map(|artist| (*artist).clone());
    match (stored, staged) {
        (Some(stored), Some(staged)) if stored.id != staged.id => {
            Err(CatalogConflict::Contradicted {
                artists: vec![stored, staged],
                detail: format!(
                    "artist '{incoming_name}' has a {source} source ID belonging to multiple library artists"
                ),
            })
        }
        (Some(_), Some(staged)) => Ok(Some(staged)),
        (Some(stored), None) => Ok(Some(stored)),
        (None, staged) => Ok(staged),
    }
}

fn matching_artist(
    incoming: &DbArtist,
    by_discogs: Option<DbArtist>,
    by_musicbrainz: Option<DbArtist>,
) -> Result<Option<DbArtist>, CatalogConflict> {
    let contradicted = |artists: Vec<DbArtist>| CatalogConflict::Contradicted {
        artists,
        detail: format!("artist '{}' has conflicting source IDs", incoming.name),
    };
    let matched = match (by_discogs, by_musicbrainz) {
        (Some(discogs), Some(musicbrainz)) if discogs.id != musicbrainz.id => {
            if !ids_agree(&discogs, incoming) || !ids_agree(&musicbrainz, incoming) {
                return Err(contradicted(vec![discogs, musicbrainz]));
            }
            let (Some(discogs_artist_id), Some(musicbrainz_artist_id)) = (
                incoming.discogs_artist_id.clone(),
                incoming.musicbrainz_artist_id.clone(),
            ) else {
                unreachable!("two catalog matches come from the credit's two catalog ids")
            };
            return Err(CatalogConflict::Identity(Box::new(
                crate::import::ArtistIdentityConflict {
                    incoming_artist_name: incoming.name.clone(),
                    discogs_artist_id,
                    musicbrainz_artist_id,
                    discogs_artist: discogs.into(),
                    musicbrainz_artist: musicbrainz.into(),
                },
            )));
        }
        (Some(artist), _) | (_, Some(artist)) => Some(artist),
        (None, None) => None,
    };
    match matched {
        Some(existing) if !ids_agree(&existing, incoming) => Err(contradicted(vec![existing])),
        matched => Ok(matched),
    }
}

/// The pending new artists of this write that share a catalog id with the
/// credit.
fn pending_catalog_indices(pending: &[DbArtist], incoming: &DbArtist) -> Vec<usize> {
    pending
        .iter()
        .enumerate()
        .filter_map(|(index, artist)| {
            let discogs_matches = incoming.discogs_artist_id.is_some()
                && incoming.discogs_artist_id == artist.discogs_artist_id;
            let musicbrainz_matches = incoming.musicbrainz_artist_id.is_some()
                && incoming.musicbrainz_artist_id == artist.musicbrainz_artist_id;
            (discogs_matches || musicbrainz_matches).then_some(index)
        })
        .collect()
}

fn stage_artist_update(
    staged_updates: &mut Vec<(String, DbArtist)>,
    artist_id: String,
    artist: DbArtist,
) {
    if let Some((_, staged)) = staged_updates
        .iter_mut()
        .find(|(staged_id, _)| *staged_id == artist_id)
    {
        *staged = artist;
    } else {
        staged_updates.push((artist_id, artist));
    }
}

fn remove_pending_artists(
    pending_artists: &mut Vec<DbArtist>,
    resolved_ids: &mut [String],
    absorbed: &[DbArtist],
    survivor_id: &str,
) {
    let absorbed_ids = absorbed
        .iter()
        .map(|artist| artist.id.as_str())
        .collect::<HashSet<_>>();
    pending_artists.retain(|artist| !absorbed_ids.contains(artist.id.as_str()));
    for resolved_id in resolved_ids {
        if absorbed_ids.contains(resolved_id.as_str()) {
            resolved_id.clear();
            resolved_id.push_str(survivor_id);
        }
    }
}

fn merge_artist_metadata(
    target: &mut DbArtist,
    incoming: &DbArtist,
) -> Result<(), ArtistWriteError> {
    if !ids_agree(target, incoming) {
        return Err(ArtistWriteError::Unresolvable(format!(
            "artist '{}' has conflicting source IDs",
            incoming.name
        )));
    }
    if target.discogs_artist_id.is_none() {
        target
            .discogs_artist_id
            .clone_from(&incoming.discogs_artist_id);
    }
    if target.musicbrainz_artist_id.is_none() {
        target
            .musicbrainz_artist_id
            .clone_from(&incoming.musicbrainz_artist_id);
    }
    if target.sort_name.is_none() {
        target.sort_name.clone_from(&incoming.sort_name);
    }
    Ok(())
}

/// Whether no catalog id the two carry contradicts the other's.
fn ids_agree(existing: &DbArtist, incoming: &DbArtist) -> bool {
    crate::import::artist_source_ids_are_compatible(
        existing,
        incoming.discogs_artist_id.as_deref(),
        incoming.musicbrainz_artist_id.as_deref(),
    )
}

#[cfg(test)]
#[path = "artist_resolution_tests.rs"]
mod tests;
