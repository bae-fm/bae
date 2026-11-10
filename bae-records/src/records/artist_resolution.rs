//! Which library artist an artist credit names — the one rule every write of
//! an artist credit decides by: an import committing its release, and a
//! metadata edit saving its artists. Each runs it inside the transaction that
//! writes the links, so it decides against the library as committed at that
//! moment and nothing another write lands in between can make it wrong.
//!
//! The rule, in order:
//!
//! 1. **A catalog id is decisive.** A credit carrying a MusicBrainz or Discogs
//!    id is the artist holding that id, as the write so far leaves the
//!    library: a library artist followed through any merge, or a new artist
//!    an earlier credit of the same write makes. One id is one artist:
//!    several library artists holding it, as two devices apart can leave
//!    them, are merged in the write, into the smallest id. Where the credit's
//!    two ids disagree — they name two different library artists, or the
//!    artist one names holds another id of the other catalog — the
//!    MusicBrainz id decides: the Discogs id is dropped from the credit and
//!    given to nothing. Where they name a new artist of the write's and
//!    another artist, and no id either holds contradicts the credit or the
//!    other, the new one is the other. MusicBrainz' and Discogs' Various
//!    Artists entries are one artist: a credit naming one by its id alone,
//!    which no artist holds, goes by both, so it is the artist holding the
//!    other's id as this rule decides for a credit carrying both.
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

/// Why a write's credits cannot be written as the library stands. Nothing of
/// the write landed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityRefusal {
    /// An artist the person picked left the library after they picked it.
    #[error("artist '{0}' no longer exists")]
    ArtistGone(String),
}

/// Why a write that decides which library artist each credit and which
/// library label each release label names was refused. Nothing of the write
/// landed; the initiator sees this and may retry once the library or the
/// statement changes.
#[derive(Debug, thiserror::Error)]
pub enum IdentityWriteRefusal {
    #[error(transparent)]
    Identity(#[from] IdentityRefusal),
    #[error(transparent)]
    StoredName(#[from] crate::stored_name::StoredNameError),
}

/// Why a write that decides which library artist each credit and which
/// library label each release label names did not commit.
pub type IdentityWriteError = WriteError<IdentityWriteRefusal>;

impl From<IdentityRefusal> for IdentityWriteError {
    fn from(refusal: IdentityRefusal) -> Self {
        Self::Refused(refusal.into())
    }
}

impl From<crate::stored_name::StoredNameError> for IdentityWriteError {
    fn from(error: crate::stored_name::StoredNameError) -> Self {
        Self::Refused(error.into())
    }
}

/// A write that resolves artist credits: [`Store::call_sql`], with an
/// [`IdentityWriteError`] the closure fails with handed back as itself.
pub async fn write_resolving_identities<R>(
    store: &Store,
    f: impl for<'ctx> FnOnce(CovenWrite<'ctx>) -> Result<R, IdentityWriteError> + Send + 'static,
) -> Result<R, IdentityWriteError>
where
    R: Send + 'static,
{
    store.write(f).await
}

/// A read that resolves artist credits: [`Store::read`], with an
/// [`IdentityWriteError`] the closure fails with handed back as itself.
#[cfg(desktop)]
pub async fn read_resolving_identities<R>(
    store: &Store,
    f: impl for<'ctx> FnOnce(SqlReadContext<'ctx>) -> Result<R, IdentityWriteError> + Send + 'static,
) -> Result<R, IdentityWriteError>
where
    R: Send + 'static,
{
    store.read_coven(f).await
}

impl Records {
    /// Resolve `credits` and write the rows the resolution inserts and fills
    /// in, in one transaction: the artist step of an import or edit, without
    /// the release around it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_or_create_artists(
        &self,
        credits: &[DbArtist],
    ) -> Result<Vec<String>, IdentityWriteError> {
        let credits = credits.to_vec();
        write_resolving_identities(&self.store, move |sql| {
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
        self.store.read(move |sql| resolve_credits_on(&sql, &credits)).await
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
    pub fn resolve_on<Q: coven::SqlRead>(
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
pub(crate) fn resolve_artists_on<Q: coven::SqlRead>(
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
                return Err(IdentityRefusal::ArtistGone(credit.id.clone()).into());
            }
            ids.push(credit.id.clone());
            continue;
        }

        let so_far = WriteSoFar { inserts: &inserts, staged: &external_id_updates };
        let (credit, matched) = settled_catalog_match_on(sql, credit, so_far)?;
        let resolved_id = match matched {
            Some(CatalogArtist { artist, folded }) => {
                let id = match artist {
                    Holder::Library(held) => {
                        let id = held.artist.id.clone();
                        for absorbed in held.absorbed {
                            merge_into(&mut merges, &mut ids, &id, absorbed, credit.created_at);
                        }
                        let mut linked = held.artist;
                        if let Some(folded) = &folded {
                            fill_missing(&mut linked, folded);
                        }
                        fill_missing(&mut linked, &credit);
                        stage_artist_update(&mut external_id_updates, id.clone(), linked);
                        id
                    }
                    Holder::New(pending) => {
                        let new = inserts
                            .iter_mut()
                            .find(|new| new.id == pending.id)
                            .expect("the catalog step names an artist this write makes");
                        if let Some(folded) = &folded {
                            fill_missing(new, folded);
                        }
                        fill_missing(new, &credit);
                        pending.id
                    }
                };
                if let Some(folded) = folded {
                    remove_pending_artists(&mut inserts, &mut ids, &[folded], &id);
                }
                id
            }
            None => {
                resolve_by_name_on(sql, &credit, &mut inserts, &mut external_id_updates, &merges)?
            }
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
pub async fn resolved_in_an_empty_library(credits: &[DbArtist]) -> Vec<String> {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let credits = credits.to_vec();
    db.read_coven(move |sql| resolve_artists_on(&sql, &credits, &HashSet::new()))
        .await
        .expect("credits resolve in an empty library")
        .ids
}

/// What the library holds for one credit as it stands now: the rule every
/// write commits by, applied to the credit alone. What the import pane shows
/// beside a credit, read inside its live query so an artist another import
/// commits reads it again.
pub fn resolve_credit_on<Q: coven::SqlRead>(
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
    let committed = WriteSoFar { inserts: &[], staged: &[] };
    let (row, matched) = settled_catalog_match_on(sql, &row, committed)?;
    Ok(match matched {
        Some(CatalogArtist { artist: Holder::Library(held), .. }) => {
            CreditResolution::Library { artist: held.artist.into() }
        }
        Some(CatalogArtist { artist: Holder::New(_), .. }) => {
            unreachable!("a read of the committed library sees no artist a write makes")
        }
        None => {
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
pub(crate) fn resolve_credits_on<'a, Q: coven::SqlRead>(
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
fn resolve_by_name_on<Q: coven::SqlRead>(
    sql: &Q,
    credit: &DbArtist,
    inserts: &mut Vec<DbArtist>,
    external_id_updates: &mut Vec<(String, DbArtist)>,
    merges: &[ArtistIdMerge],
) -> Result<String, DbError> {
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
        fill_missing(&mut inserts[*only], credit);
        return Ok(inserts[*only].id.clone());
    }
    if pending.is_empty() {
        // An artist this write merges into another is that one.
        let mut named = name_matches_on(sql, credit, external_id_updates)?;
        named.retain(|artist| !merges.iter().any(|merge| merge.absorbed.id == artist.id));
        if let [only] = named.as_slice() {
            let mut linked = only.clone();
            fill_missing(&mut linked, credit);
            let id = linked.id.clone();
            stage_artist_update(external_id_updates, id.clone(), linked);
            return Ok(id);
        }
    }
    inserts.push(credit.clone());
    Ok(credit.id.clone())
}

/// `links` with each one's artist rewritten from the credit id it names to
/// the library artist that credit resolved to. Every link names a credit of
/// the write that resolved `artist_ids`, so one naming another is a bug of
/// the write's, which panics in the store's work.
pub fn relink<T: Clone>(
    links: &[T],
    artist_ids: &HashMap<String, String>,
    label: &str,
    artist_id: impl Fn(&mut T) -> &mut String,
) -> Vec<T> {
    links
        .iter()
        .map(|link| {
            let mut link = link.clone();
            let credit_id = artist_id(&mut link);
            let resolved = artist_ids.get(credit_id.as_str()).unwrap_or_else(|| {
                panic!("{label} names {credit_id}, which no credit of the write is")
            });
            credit_id.clone_from(resolved);
            link
        })
        .collect()
}

/// The album's credits, resolved. Each place keeps its credit even where two
/// resolve to one artist: the list names that artist twice.
pub fn relink_album_artists(
    links: &[DbAlbumArtist],
    artist_ids: &HashMap<String, String>,
) -> Vec<DbAlbumArtist> {
    relink(links, artist_ids, "album artist", |link| &mut link.artist_id)
}

/// Each track's own credits, resolved, every place kept.
pub fn relink_track_artists(
    links: &[DbTrackArtist],
    artist_ids: &HashMap<String, String>,
) -> Vec<DbTrackArtist> {
    relink(links, artist_ids, "track artist", |link| &mut link.artist_id)
}

/// What this write has decided before the credit it resolves now: the new
/// artists it makes, and the library artists whose ids it fills in. The
/// catalog-id step sees the library as these leave it.
#[derive(Clone, Copy)]
struct WriteSoFar<'a> {
    inserts: &'a [DbArtist],
    staged: &'a [(String, DbArtist)],
}

/// What the catalog-id step says about one credit.
enum CatalogMatch {
    /// The credit carries no id an artist holds.
    Unmatched,
    /// The artist its ids name.
    Artist(CatalogArtist),
    /// Its two ids disagree: they name two artists that are not one, or the
    /// artist one names holds another id of the other catalog.
    Disagrees,
}

/// The artist a credit's ids name, and a new artist of this write that the
/// credit says is that one too.
struct CatalogArtist {
    artist: Holder,
    /// A new artist this write makes holding the credit's other id. It is not
    /// in the library yet, and no id it holds contradicts the credit or
    /// `artist`, so the credit makes it `artist`: it takes the place of every
    /// credit this write already resolved to it, and gives `artist` its ids.
    folded: Option<DbArtist>,
}

/// The artist one catalog id names, as this write leaves the library.
enum Holder {
    /// A library artist.
    Library(HeldArtist),
    /// A new artist this write makes.
    New(DbArtist),
}

impl Holder {
    fn artist(&self) -> &DbArtist {
        match self {
            Self::Library(held) => &held.artist,
            Self::New(artist) => artist,
        }
    }
}

/// The library artist a catalog id names, and the other library artists
/// holding it, which the write merges into it.
struct HeldArtist {
    artist: DbArtist,
    absorbed: Vec<DbArtist>,
}

/// The catalog-id step: the artist the credit's Discogs and MusicBrainz ids
/// name, as this write so far leaves the library.
fn catalog_match_on<Q: coven::SqlRead>(
    sql: &Q,
    credit: &DbArtist,
    so_far: WriteSoFar<'_>,
) -> Result<CatalogMatch, DbError> {
    let by_discogs =
        holder_on(sql, CatalogColumn::Discogs, credit.discogs_artist_id.as_deref(), so_far)?;
    let by_musicbrainz = holder_on(
        sql,
        CatalogColumn::MusicBrainz,
        credit.musicbrainz_artist_id.as_deref(),
        so_far,
    )?;
    let agrees = |holder: &Holder| ids_agree(holder.artist(), credit);
    Ok(match (by_discogs, by_musicbrainz) {
        (None, None) => CatalogMatch::Unmatched,
        (Some(one), None) | (None, Some(one)) => match agrees(&one) {
            true => CatalogMatch::Artist(CatalogArtist { artist: one, folded: None }),
            false => CatalogMatch::Disagrees,
        },
        (Some(Holder::Library(discogs)), Some(Holder::Library(musicbrainz)))
            if discogs.artist.id == musicbrainz.artist.id =>
        {
            let mut absorbed = discogs.absorbed;
            absorbed.extend(musicbrainz.absorbed);
            let artist = Holder::Library(HeldArtist { artist: musicbrainz.artist, absorbed });
            match agrees(&artist) {
                true => CatalogMatch::Artist(CatalogArtist { artist, folded: None }),
                false => CatalogMatch::Disagrees,
            }
        }
        (Some(Holder::New(discogs)), Some(Holder::New(musicbrainz)))
            if discogs.id == musicbrainz.id =>
        {
            let artist = Holder::New(musicbrainz);
            match agrees(&artist) {
                true => CatalogMatch::Artist(CatalogArtist { artist, folded: None }),
                false => CatalogMatch::Disagrees,
            }
        }
        // Two artists, one of them a new artist of this write's: it folds
        // into the other where nothing either holds contradicts the credit or
        // the other. Two library artists stay two.
        (Some(Holder::New(new)), Some(artist)) | (Some(artist), Some(Holder::New(new)))
            if agrees(&artist) && ids_agree(&new, credit) && ids_agree(&new, artist.artist()) =>
        {
            CatalogMatch::Artist(CatalogArtist { artist, folded: Some(new) })
        }
        (Some(_), Some(_)) => CatalogMatch::Disagrees,
    })
}

/// The artist holding the catalog id `id` of `column`, as this write so far
/// leaves the library: a new artist it makes, else the library artist
/// [`exact_artist`] names. No id is held by both, since a new artist is made
/// only of ids no artist holds.
fn holder_on<Q: coven::SqlRead>(
    sql: &Q,
    column: CatalogColumn,
    id: Option<&str>,
    so_far: WriteSoFar<'_>,
) -> Result<Option<Holder>, DbError> {
    let Some(id) = id else {
        return Ok(None);
    };
    if let Some(new) = so_far.inserts.iter().find(|new| column.of(new) == Some(id)) {
        return Ok(Some(Holder::New(new.clone())));
    }
    let stored = artists_by_catalog_id_on(sql, column, id)?;
    Ok(exact_artist(id, stored, so_far.staged, column).map(Holder::Library))
}

/// The credit with the other catalog's Various Artists id beside the one it
/// names Various Artists by: MusicBrainz' and Discogs' Various Artists
/// entries are one artist. None for any other credit.
fn with_both_various_artists_ids(credit: &DbArtist) -> Option<DbArtist> {
    if !credit.is_various_artists() {
        return None;
    }
    let va = &crate::VARIOUS_ARTISTS;
    match (credit.discogs_artist_id.as_deref(), credit.musicbrainz_artist_id.as_deref()) {
        (Some(_), None) => Some(DbArtist {
            musicbrainz_artist_id: Some(va.musicbrainz.to_string()),
            ..credit.clone()
        }),
        (None, Some(_)) => {
            Some(DbArtist { discogs_artist_id: Some(va.discogs.to_string()), ..credit.clone() })
        }
        (Some(_), Some(_)) | (None, None) => None,
    }
}

/// The credit as the catalog-id step settles it, and the artist its ids name.
/// A credit naming Various Artists by one catalog's id, which no artist
/// holds, goes by both catalogs' Various Artists ids. Where its two ids
/// disagree, the MusicBrainz id decides: the Discogs id is dropped from the
/// credit, which then goes by its MusicBrainz id alone, and nothing is given
/// the Discogs id.
fn settled_catalog_match_on<Q: coven::SqlRead>(
    sql: &Q,
    credit: &DbArtist,
    so_far: WriteSoFar<'_>,
) -> Result<(DbArtist, Option<CatalogArtist>), DbError> {
    let (credit, matched) = match catalog_match_on(sql, credit, so_far)? {
        CatalogMatch::Unmatched => match with_both_various_artists_ids(credit) {
            Some(both) => match catalog_match_on(sql, &both, so_far)? {
                CatalogMatch::Unmatched => (credit.clone(), CatalogMatch::Unmatched),
                matched => (both, matched),
            },
            None => (credit.clone(), CatalogMatch::Unmatched),
        },
        matched => (credit.clone(), matched),
    };
    match matched {
        CatalogMatch::Unmatched => Ok((credit, None)),
        CatalogMatch::Artist(found) => Ok((credit, Some(found))),
        CatalogMatch::Disagrees => {
            let settled = DbArtist { discogs_artist_id: None, ..credit };
            match catalog_match_on(sql, &settled, so_far)? {
                CatalogMatch::Unmatched => Ok((settled, None)),
                CatalogMatch::Artist(found) => Ok((settled, Some(found))),
                CatalogMatch::Disagrees => {
                    unreachable!("a credit carrying one catalog id agrees with what it names")
                }
            }
        }
    }
}

/// Every shown library artist whose folded name is the credit's and whose
/// catalog ids — as this write's own fills leave them — agree with the
/// credit's. Absorbed artists are left out: a merge shows them as their
/// survivor, which is matched on its own name.
pub(super) fn name_matches_on<Q: coven::SqlRead>(
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

impl CatalogColumn {
    /// The id of this catalog `artist` holds.
    fn of(self, artist: &DbArtist) -> Option<&str> {
        match self {
            Self::Discogs => artist.discogs_artist_id.as_deref(),
            Self::MusicBrainz => artist.musicbrainz_artist_id.as_deref(),
        }
    }
}

/// Every artist whose `column` holds `catalog_id`, or whose row the catalog
/// id names (its column may have been filled in or edited since), followed
/// through any merge to the artist it shows as. More than one is a library
/// holding one catalog entry on several artists.
fn artists_by_catalog_id_on<Q: coven::SqlRead>(
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

fn artist_by_id_on<Q: coven::SqlRead>(
    sql: &Q,
    artist_id: &str,
) -> Result<Option<DbArtist>, DbError> {
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
    let staged = |id: &str| {
        staged_updates.iter().find(|(staged_id, _)| staged_id == id).map(|(_, artist)| artist)
    };
    let mut named: Vec<DbArtist> =
        stored.into_iter().map(|artist| staged(&artist.id).cloned().unwrap_or(artist)).collect();
    for (id, artist) in staged_updates {
        if column.of(artist) == Some(source_id) && !named.iter().any(|named| named.id == *id) {
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
