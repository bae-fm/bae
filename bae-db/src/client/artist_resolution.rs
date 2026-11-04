//! Which library artist an artist credit names — the one rule every write of
//! an artist credit decides by: an import committing its release, and a
//! metadata edit saving its artists. Each runs it inside the transaction that
//! writes the links, so it decides against the library as committed at that
//! moment and nothing another write lands in between can make it wrong.
//!
//! The rule, in order:
//!
//! 1. **A catalog id is decisive.** A credit carrying a MusicBrainz or Discogs
//!    id is the library artist holding that id, followed through any merge.
//!    One id is one artist: several library artists holding it, as two
//!    devices apart can leave them, are merged in the write, into the
//!    smallest id. Two different artists holding the credit's two ids is the
//!    conflict a person settles on the import page; a matched artist holding a
//!    different id is refused, and the import page offers it to pick.
//! 2. **Otherwise the name.** A credit is the one library artist whose
//!    `name_key`, or one of whose other printed names (`artist_credits`),
//!    equals the credit's folded name and whose catalog ids do not contradict
//!    the credit's. Several such artists leave it ambiguous, which
//!    commits as a new artist; none commits as a new artist.
//!
//! A credit linked to an existing artist fills in that artist's missing catalog
//! ids and sort name. The artist keeps its own id: row ids are never rewritten,
//! so an artist first created from a name alone keeps the independent id it was
//! created with after a later credit gives it a catalog id. Only a *new*
//! artist a catalog names takes the id [`crate::identity::artist_id`]
//! derives from that catalog entry.

use super::*;

/// Why a write that decides which library artist each credit and which
/// library label each release label names did not commit. Nothing of the
/// write landed; the initiator sees this and may retry once the library or
/// the statement changes.
#[derive(Debug, thiserror::Error)]
pub enum IdentityWriteError {
    /// Two library artists each hold one of the credit's catalog ids — the
    /// conflict a person resolves on the import page.
    #[error(transparent)]
    ArtistIdentityConflict(Box<crate::ArtistIdentityConflict>),
    /// Two library labels each hold one of the label's catalog ids — the
    /// conflict a person resolves on the import page.
    #[error(transparent)]
    LabelIdentityConflict(Box<crate::LabelIdentityConflict>),
    /// The statement cannot be linked to the library as it stands: its ids
    /// contradict the artist or label they match, or a picked artist no
    /// longer exists.
    #[error("{0}")]
    Unresolvable(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// Carry an [`IdentityWriteError`] out of a coven write closure with its type
/// intact, so [`Database::identity_write_error`] can hand it back.
pub fn identity_write_failure(error: IdentityWriteError) -> CovenError {
    match error {
        IdentityWriteError::Db(error) => CovenError::from(error),
        other => rusqlite::Error::UserFunctionError(Box::new(other)).into(),
    }
}

impl Database {
    /// A write that resolves artist credits: [`Self::call_sql`], with an
    /// [`IdentityWriteError`] the closure fails with handed back as itself.
    pub async fn write_resolving_identities<R>(
        &self,
        f: impl for<'ctx> FnOnce(CovenWrite<'ctx>) -> Result<R, IdentityWriteError> + Send + 'static,
    ) -> Result<R, IdentityWriteError>
    where
        R: Send + 'static,
    {
        self.inner
            .handle
            .write(move |sql| f(sql).map_err(identity_write_failure))
            .await
            .map_err(Self::identity_write_error)
    }

    /// A read that resolves artist credits: [`Self::read`], with an
    /// [`IdentityWriteError`] the closure fails with handed back as itself.
    #[cfg(desktop)]
    pub async fn read_resolving_identities<R>(
        &self,
        f: impl for<'ctx> FnOnce(SqlReadContext<'ctx>) -> Result<R, IdentityWriteError> + Send + 'static,
    ) -> Result<R, IdentityWriteError>
    where
        R: Send + 'static,
    {
        self.inner
            .handle
            .read(move |sql| f(sql).map_err(identity_write_failure))
            .await
            .map_err(Self::identity_write_error)
    }

    /// Resolve `credits` and write the rows the resolution inserts and fills
    /// in, in one transaction: the artist step of an import or edit, without
    /// the release around it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_or_create_artists(
        &self,
        credits: &[DbArtist],
    ) -> Result<Vec<String>, IdentityWriteError> {
        let credits = credits.to_vec();
        self.write_resolving_identities(move |sql| {
            let resolved = ArtistCredits { credits: &credits, picked: &[] }.resolve_on(&sql)?;
            resolved.write_on(&sql)?;
            Ok(resolved.ids)
        })
        .await
    }

    /// What the library holds for each of `credits` as it stands now.
    pub async fn resolve_artist_credits(
        &self,
        credits: &[bae_catalogs::credits::ArtistCredit],
    ) -> Result<Vec<crate::ResolvedCredit>, DbError> {
        let credits = credits.to_vec();
        self.read(move |sql| resolve_credits_on(&sql, &credits)).await
    }

    /// The [`IdentityWriteError`] a coven write closure failed with, or the
    /// database error it failed with otherwise.
    pub fn identity_write_error(error: CovenError) -> IdentityWriteError {
        match error {
            CovenError::Database(coven::DbError::Sqlite(rusqlite::Error::UserFunctionError(
                host,
            ))) => match host.downcast::<IdentityWriteError>() {
                Ok(error) => *error,
                Err(other) => IdentityWriteError::Db(Self::coven_error(
                    rusqlite::Error::UserFunctionError(other).into(),
                )),
            },
            other => IdentityWriteError::Db(Self::coven_error(other)),
        }
    }
}

impl IdentityWriteError {
    /// This error as a database error, for a fixture's write, which no person
    /// answers a conflict for.
    pub(super) fn into_db_error(self) -> DbError {
        match self {
            Self::Db(error) => error,
            other => DbError::Message(other.to_string()),
        }
    }
}

impl From<rusqlite::Error> for IdentityWriteError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(error.into())
    }
}

/// The resolved artist of every credit an import or edit writes, in input
/// order, with the rows the same write inserts or fills in.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedArtists {
    /// The library id each input credit resolved to, positionally.
    pub ids: Vec<String>,
    /// New artists to insert, each with its final id.
    pub inserts: Vec<DbArtist>,
    /// `(artist_id, artist)` — an existing artist whose empty catalog ids and
    /// sort name this write fills in.
    pub external_id_updates: Vec<(String, DbArtist)>,
    /// Library artists holding one catalog id, each merged into the artist
    /// with the smallest id.
    pub merges: Vec<ArtistIdMerge>,
}

/// A library artist this write merges into another because both hold one
/// catalog id.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtistIdMerge {
    pub survivor: String,
    /// The absorbed artist as this write leaves it, whose ids and sort name
    /// the survivor takes where it has none.
    pub absorbed: DbArtist,
    /// When the credit that found the two was written.
    pub at: DateTime<Utc>,
}

impl ResolvedArtists {
    /// Insert the new artists and fill in the linked ones, before any link
    /// that points at them is written.
    pub fn write_on(&self, tx: &CovenWrite<'_>) -> Result<(), DbError> {
        for artist in &self.inserts {
            insert_artist_row(tx, artist)?;
        }
        for (artist_id, artist) in &self.external_id_updates {
            update_artist_external_ids_row(
                tx,
                artist_id,
                artist.discogs_artist_id.as_deref(),
                artist.musicbrainz_artist_id.as_deref(),
                artist.sort_name.as_deref(),
            )?;
        }
        for merge in &self.merges {
            super::identity_merges::merge_artist_on(
                tx,
                &super::identity_merges::ArtistMerge {
                    survivor: &merge.survivor,
                    absorbed: &merge.absorbed.id,
                    discogs_artist_id: merge.absorbed.discogs_artist_id.as_deref(),
                    musicbrainz_artist_id: merge.absorbed.musicbrainz_artist_id.as_deref(),
                    sort_name: merge.absorbed.sort_name.as_deref(),
                    at: &merge.at.to_rfc3339(),
                },
            )?;
        }
        Ok(())
    }

    /// The resolved id of every input credit, keyed by the id it came in with.
    pub fn id_map(&self, credits: &[DbArtist]) -> HashMap<String, String> {
        credits.iter().zip(&self.ids).map(|(credit, id)| (credit.id.clone(), id.clone())).collect()
    }
}

/// The artist credits one write links, as its links name them. A credit whose
/// id is in `picked` is a library artist a person chose; every other credit is
/// what a source or a person said, resolved by `resolve_artists_on` when the
/// write commits.
#[derive(Debug, Default, Clone, Copy)]
pub struct ArtistCredits<'a> {
    pub credits: &'a [DbArtist],
    pub picked: &'a [String],
}

impl ArtistCredits<'_> {
    pub fn resolve_on<Q: QueryOne + QueryRows>(
        &self,
        sql: &Q,
    ) -> Result<ResolvedArtists, IdentityWriteError> {
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
) -> Result<ResolvedArtists, IdentityWriteError> {
    let mut ids = Vec::with_capacity(credits.len());
    let mut inserts: Vec<DbArtist> = Vec::new();
    let mut external_id_updates: Vec<(String, DbArtist)> = Vec::new();
    let mut merges: Vec<ArtistIdMerge> = Vec::new();

    for credit in credits {
        if picked.contains(&credit.id) {
            if artist_by_id_on(sql, &credit.id)?.is_none() {
                return Err(IdentityWriteError::Unresolvable(format!(
                    "artist '{}' no longer exists",
                    credit.id
                )));
            }
            ids.push(credit.id.clone());
            continue;
        }

        let committed = match catalog_match_on(sql, credit, &external_id_updates)? {
            CatalogMatch::Unmatched => None,
            CatalogMatch::Artist(held) => {
                for absorbed in held.absorbed {
                    merge_into(&mut merges, &mut ids, &held.artist.id, absorbed, credit.created_at);
                }
                Some(held.artist)
            }
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
                None => resolve_by_name_on(
                    sql,
                    credit,
                    &mut inserts,
                    &mut external_id_updates,
                    &merges,
                )?,
            },
        };
        ids.push(resolved_id);
    }

    // A new artist a catalog names gets the id that catalog entry names, so
    // every device that meets this artist writes one row.
    for insert in &mut inserts {
        let Some(identity) = crate::identity::artist_id(
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

    Ok(ResolvedArtists { ids, inserts, external_id_updates, merges })
}

/// The library artist each of `credits` resolves to when one write commits
/// them all into an empty library, positionally: which credits of a release
/// the commit makes one artist.
#[cfg(any(test, feature = "test-utils"))]
pub fn resolved_in_an_empty_library(credits: &[DbArtist]) -> Vec<String> {
    let conn = crate::schema::bare_library_connection(crate::test_fixtures::TEST_SCHEMA);
    resolve_artists_on(&conn, credits, &HashSet::new())
        .expect("credits resolve in an empty library")
        .ids
}

/// What the library holds for one credit as it stands now: the rule every
/// write commits by, applied to the credit alone. What the import pane shows
/// beside a credit, read inside its live query so an artist another import
/// commits reads it again.
pub fn resolve_credit_on<Q: QueryOne + QueryRows>(
    sql: &Q,
    credit: &bae_catalogs::credits::ArtistCredit,
) -> Result<crate::CreditResolution, DbError> {
    use crate::CreditResolution;
    let row = DbArtist {
        id: String::new(),
        name: credit.name.clone(),
        sort_name: credit.sort_name.clone(),
        discogs_artist_id: credit.discogs_artist_id.clone(),
        musicbrainz_artist_id: credit.musicbrainz_artist_id.clone(),
        created_at: chrono::DateTime::<Utc>::MIN_UTC,
    };
    Ok(match catalog_match_on(sql, &row, &[])? {
        CatalogMatch::Artist(held) => CreditResolution::Library { artist: held.artist.into() },
        CatalogMatch::Conflict(conflict) => {
            CreditResolution::Conflicting { artists: conflict.artists() }
        }
        CatalogMatch::Unmatched => {
            let mut named = name_matches_on(sql, &row, &[])?;
            match named.len() {
                0 => CreditResolution::New,
                1 => CreditResolution::Library { artist: named.remove(0).into() },
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
    credits: impl IntoIterator<Item = &'a bae_catalogs::credits::ArtistCredit>,
) -> Result<Vec<crate::ResolvedCredit>, DbError> {
    resolve_credits_with(credits, |credit| resolve_credit_on(sql, credit))
}

pub fn resolve_credits_with<'a>(
    credits: impl IntoIterator<Item = &'a bae_catalogs::credits::ArtistCredit>,
    mut resolve: impl FnMut(
        &bae_catalogs::credits::ArtistCredit,
    ) -> Result<crate::CreditResolution, DbError>,
) -> Result<Vec<crate::ResolvedCredit>, DbError> {
    let mut resolved: Vec<crate::ResolvedCredit> = Vec::new();
    for credit in credits {
        if resolved.iter().any(|known| known.credit == *credit) {
            continue;
        }
        resolved
            .push(crate::ResolvedCredit { credit: credit.clone(), resolution: resolve(credit)? });
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
    merges: &[ArtistIdMerge],
) -> Result<String, IdentityWriteError> {
    let pending: Vec<usize> = inserts
        .iter()
        .enumerate()
        .filter(|(_, pending)| {
            bae_types::text_match::same_artist_name(&pending.name, &credit.name)
                && ids_agree(pending, credit)
        })
        .map(|(index, _)| index)
        .collect();
    if let [only] = pending.as_slice() {
        merge_artist_metadata(&mut inserts[*only], credit)?;
        return Ok(inserts[*only].id.clone());
    }
    if pending.is_empty() {
        // An artist this write merges into another is that one.
        let mut named = name_matches_on(sql, credit, external_id_updates)?;
        named.retain(|artist| !merges.iter().any(|merge| merge.absorbed.id == artist.id));
        if let [only] = named.as_slice() {
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
pub fn relink<T: Clone>(
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

/// The album's credits, resolved. Each place keeps its credit even where two
/// resolve to one artist: the list names that artist twice.
pub fn relink_album_artists(
    links: &[DbAlbumArtist],
    artist_ids: &HashMap<String, String>,
) -> Result<Vec<DbAlbumArtist>, DbError> {
    relink(links, artist_ids, "album artist", |link| &mut link.artist_id)
}

/// Each track's own credits, resolved, every place kept.
pub fn relink_track_artists(
    links: &[DbTrackArtist],
    artist_ids: &HashMap<String, String>,
) -> Result<Vec<DbTrackArtist>, DbError> {
    relink(links, artist_ids, "track artist", |link| &mut link.artist_id)
}

/// What the catalog-id step says about one credit.
pub(super) enum CatalogMatch {
    /// The credit carries no id the library holds.
    Unmatched,
    /// The library artist its ids name.
    Artist(HeldArtist),
    /// Its ids name library artists that disagree with it.
    Conflict(CatalogConflict),
}

pub(super) enum CatalogConflict {
    /// Two different artists each hold one of the credit's ids.
    Identity(Box<crate::ArtistIdentityConflict>),
    /// A matched artist holds a different id of the same catalog.
    Contradicted { artists: Vec<DbArtist>, detail: String },
}

impl CatalogConflict {
    fn into_error(self) -> IdentityWriteError {
        match self {
            Self::Identity(conflict) => IdentityWriteError::ArtistIdentityConflict(conflict),
            Self::Contradicted { detail, .. } => IdentityWriteError::Unresolvable(detail),
        }
    }

    /// The library artists the credit's ids point at.
    fn artists(self) -> Vec<crate::ExistingArtist> {
        match self {
            Self::Identity(conflict) => {
                let crate::ArtistIdentityConflict { discogs_artist, musicbrainz_artist, .. } =
                    *conflict;
                vec![discogs_artist, musicbrainz_artist]
            }
            Self::Contradicted { artists, .. } => artists.into_iter().map(Into::into).collect(),
        }
    }
}

/// The library artist a catalog id names, and the other library artists
/// holding it, which the write merges into it.
pub(super) struct HeldArtist {
    pub(super) artist: DbArtist,
    pub(super) absorbed: Vec<DbArtist>,
}

/// The catalog-id step: the library artist the credit's Discogs and
/// MusicBrainz ids name, as `staged` (this write's own fills) leaves it.
pub(super) fn catalog_match_on<Q: QueryRows>(
    sql: &Q,
    credit: &DbArtist,
    staged: &[(String, DbArtist)],
) -> Result<CatalogMatch, DbError> {
    let mut absorbed = Vec::new();
    let mut held_by = |column: CatalogColumn, id: Option<&str>| -> Result<_, DbError> {
        let Some(id) = id else {
            return Ok(None);
        };
        let held = exact_artist(id, artists_by_catalog_id_on(sql, column, id)?, staged, column);
        Ok(held.map(|held| {
            absorbed.extend(held.absorbed);
            held.artist
        }))
    };
    let by_discogs = held_by(CatalogColumn::Discogs, credit.discogs_artist_id.as_deref())?;
    let by_musicbrainz =
        held_by(CatalogColumn::MusicBrainz, credit.musicbrainz_artist_id.as_deref())?;
    let matched = match matching_artist(credit, by_discogs, by_musicbrainz) {
        Ok(matched) => matched,
        Err(conflict) => return Ok(CatalogMatch::Conflict(conflict)),
    };
    if let Some(artist) = matched {
        return Ok(CatalogMatch::Artist(HeldArtist { artist, absorbed }));
    }
    if !credit.is_various_artists() {
        return Ok(CatalogMatch::Unmatched);
    }

    // The two provider IDs are one canonical cross-provider identity. An
    // incoming exact provider match still wins; only its absence permits the
    // other provider's canonical row to stand in for it.
    let va = &crate::VARIOUS_ARTISTS;
    let (column, id) =
        match (credit.discogs_artist_id.as_deref(), credit.musicbrainz_artist_id.as_deref()) {
            (Some(_), None) => (CatalogColumn::MusicBrainz, va.musicbrainz),
            (None, Some(_)) => (CatalogColumn::Discogs, va.discogs),
            (Some(_), Some(_)) | (None, None) => return Ok(CatalogMatch::Unmatched),
        };
    Ok(match exact_artist(id, artists_by_catalog_id_on(sql, column, id)?, staged, column) {
        Some(held) => CatalogMatch::Artist(held),
        None => CatalogMatch::Unmatched,
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
    let key = bae_types::text_match::artist_name_key(&credit.name);
    if key.is_empty() {
        return Ok(Vec::new());
    }
    // An artist goes by its own name and by each of its other printed names;
    // a printed name an absorbed artist held names the artist it shows as.
    let named = sql.query(
        &format!(
            "SELECT * FROM artists WHERE {shown} AND (name_key = ?1 OR id IN ( \
                 SELECT {credit_artist} FROM artist_credits credit_name \
                 WHERE credit_name.name_key = ?1)) \
             ORDER BY id",
            shown = artist_is_shown("artists"),
            credit_artist = shown_artist_id("credit_name.artist_id"),
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

/// Every artist whose `column` holds `catalog_id`, or whose row the catalog
/// id names (its column may have been filled in or edited since), followed
/// through any merge to the artist it shows as. More than one is a library
/// holding one catalog entry on several artists.
fn artists_by_catalog_id_on<Q: QueryRows>(
    sql: &Q,
    column: CatalogColumn,
    catalog_id: &str,
) -> Result<Vec<DbArtist>, DbError> {
    let (column, identity) = match column {
        CatalogColumn::Discogs => {
            ("discogs_artist_id", crate::identity::artist_id(None, Some(catalog_id)))
        }
        CatalogColumn::MusicBrainz => {
            ("musicbrainz_artist_id", crate::identity::artist_id(Some(catalog_id), None))
        }
    };
    sql.query(
        &format!(
            "SELECT a.* FROM artists a WHERE a.id IN ( \
                 SELECT {} FROM artists named \
                 WHERE named.{column} = ?1 OR named.id = ?2) \
             ORDER BY a.id",
            shown_artist_id("named.id")
        ),
        params![catalog_id, identity],
        row_to_artist,
    )
    .map_err(DbError::from)
}

fn artist_by_id_on<Q: QueryOne>(sql: &Q, artist_id: &str) -> Result<Option<DbArtist>, DbError> {
    sql.query_row("SELECT * FROM artists WHERE id = ?", params![artist_id], row_to_artist)
        .optional()
        .map_err(DbError::from)
}

/// The one library artist a credit's `column` id `source_id` names, as this
/// write's own fills (`staged_updates`) leave it: of the artists the library
/// holds it on (`stored`) and those this write is giving it, the smallest id.
/// One id is one artist, so the others are merged into it, and it takes the
/// ids and sort name it lacks from them. None is no match.
fn exact_artist(
    source_id: &str,
    stored: Vec<DbArtist>,
    staged_updates: &[(String, DbArtist)],
    column: CatalogColumn,
) -> Option<HeldArtist> {
    let external_id = |artist: &DbArtist| match column {
        CatalogColumn::Discogs => artist.discogs_artist_id.clone(),
        CatalogColumn::MusicBrainz => artist.musicbrainz_artist_id.clone(),
    };
    let staged = |id: &str| {
        staged_updates.iter().find(|(staged_id, _)| staged_id == id).map(|(_, artist)| artist)
    };
    let mut named: Vec<DbArtist> =
        stored.into_iter().map(|artist| staged(&artist.id).cloned().unwrap_or(artist)).collect();
    for (id, artist) in staged_updates {
        if external_id(artist).as_deref() == Some(source_id)
            && !named.iter().any(|named| named.id == *id)
        {
            named.push(artist.clone());
        }
    }
    named.sort_by(|a, b| a.id.cmp(&b.id));
    let mut named = named.into_iter();
    let mut artist = named.next()?;
    let absorbed: Vec<DbArtist> = named.collect();
    for other in &absorbed {
        fill_missing(&mut artist, other);
    }
    Some(HeldArtist { artist, absorbed })
}

/// Record that `absorbed` is merged into `survivor`, once, and name the
/// survivor wherever this write already resolved a credit to `absorbed`.
fn merge_into(
    merges: &mut Vec<ArtistIdMerge>,
    ids: &mut [String],
    survivor: &str,
    absorbed: DbArtist,
    at: DateTime<Utc>,
) {
    for id in ids.iter_mut().filter(|id| **id == absorbed.id) {
        *id = survivor.to_string();
    }
    if !merges.iter().any(|merge| merge.absorbed.id == absorbed.id) {
        merges.push(ArtistIdMerge { survivor: survivor.to_string(), absorbed, at });
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
            let (Some(discogs_artist_id), Some(musicbrainz_artist_id)) =
                (incoming.discogs_artist_id.clone(), incoming.musicbrainz_artist_id.clone())
            else {
                unreachable!("two catalog matches come from the credit's two catalog ids")
            };
            return Err(CatalogConflict::Identity(Box::new(crate::ArtistIdentityConflict {
                incoming_artist_name: incoming.name.clone(),
                discogs_artist_id,
                musicbrainz_artist_id,
                discogs_artist: discogs.into(),
                musicbrainz_artist: musicbrainz.into(),
            })));
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
    if let Some((_, staged)) =
        staged_updates.iter_mut().find(|(staged_id, _)| *staged_id == artist_id)
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
    let absorbed_ids = absorbed.iter().map(|artist| artist.id.as_str()).collect::<HashSet<_>>();
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
) -> Result<(), IdentityWriteError> {
    if !ids_agree(target, incoming) {
        return Err(IdentityWriteError::Unresolvable(format!(
            "artist '{}' has conflicting source IDs",
            incoming.name
        )));
    }
    fill_missing(target, incoming);
    Ok(())
}

/// Give `target` each catalog id and the sort name it lacks from `other`.
fn fill_missing(target: &mut DbArtist, other: &DbArtist) {
    if target.discogs_artist_id.is_none() {
        target.discogs_artist_id.clone_from(&other.discogs_artist_id);
    }
    if target.musicbrainz_artist_id.is_none() {
        target.musicbrainz_artist_id.clone_from(&other.musicbrainz_artist_id);
    }
    if target.sort_name.is_none() {
        target.sort_name.clone_from(&other.sort_name);
    }
}

/// Whether no catalog id the two carry contradicts the other's.
fn ids_agree(existing: &DbArtist, incoming: &DbArtist) -> bool {
    crate::artist_source_ids_are_compatible(
        existing,
        incoming.discogs_artist_id.as_deref(),
        incoming.musicbrainz_artist_id.as_deref(),
    )
}

#[cfg(test)]
#[path = "artist_resolution_tests.rs"]
mod tests;
