//! The catalog-id step of the rule every write naming artists or labels
//! decides by: which library row a credit's MusicBrainz and Discogs ids name,
//! as the write so far leaves the library, and how ids that disagree settle.
//! An artist credit names an artist (`artist_resolution`) and a release's
//! label names a label (`label_resolution`) by the same two catalogs' ids,
//! so one step decides both; each domain applies what it decides.
//!
//! A credit carrying a catalog id is the row holding that id: a library row
//! followed through any merge, or a new row an earlier credit of the same
//! write makes. One id is one row: several library rows holding it are one, the
//! smallest id, which the write merges the others into. Where the credit's two
//! ids disagree — they name two rows that are not one, or the row one names
//! holds another id of the other catalog — the MusicBrainz id decides: the
//! Discogs id is dropped from the credit and given to nothing. Where they name a new row of the
//! write's and another row, and no id either holds contradicts the credit or
//! the other, the new one is the other.
//!
//! A new row takes its id once the write has decided every credit
//! ([`identify_new_rows`]): a later credit of the write can give it a catalog
//! id it lacks, and the id its catalog entry derives goes by MusicBrainz
//! first.

use super::*;

/// The catalogs whose ids name an artist or a label.
#[derive(Clone, Copy)]
pub(super) enum CatalogColumn {
    Discogs,
    MusicBrainz,
}

/// What names an artist or a label by catalog ids: a library row, or a
/// credit naming one.
pub(super) trait CatalogIds {
    /// The id of `column`'s catalog it carries.
    fn catalog_id(&self, column: CatalogColumn) -> Option<&str>;
}

/// Whether no catalog id the two carry contradicts the other's.
pub(super) fn ids_agree(a: &impl CatalogIds, b: &impl CatalogIds) -> bool {
    [CatalogColumn::Discogs, CatalogColumn::MusicBrainz].into_iter().all(|column| {
        crate::models::source_ids_are_compatible(a.catalog_id(column), b.catalog_id(column))
    })
}

/// A library row catalog ids name: an artist, or a label.
pub(super) trait CatalogRow: CatalogIds + Clone {
    fn row_id(&self) -> &str;

    fn set_row_id(&mut self, id: String);

    /// The id of the row a catalog entry names, preferring MusicBrainz, so
    /// every device that meets it writes one row. None for a row no catalog
    /// names.
    fn catalog_entry_id(musicbrainz: Option<&str>, discogs: Option<&str>) -> Option<String>;

    /// Take each catalog id this row lacks from `other`, a row or credit of
    /// the same artist or label, and whatever else such a row takes from it.
    fn fill_missing(&mut self, other: &Self);

    /// Every library row whose `column` holds `catalog_id`, or whose row the
    /// catalog id names (its column may have been filled in or edited
    /// since), followed through any merge to the row it shows as. More than
    /// one is a library holding one catalog entry on several.
    fn holding_on<Q: coven::SqlRead>(
        sql: &Q,
        column: CatalogColumn,
        catalog_id: &str,
    ) -> Result<Vec<Self>, DbError>;
}

/// What names a library row by its catalog ids: an artist credit, or a
/// release's label.
pub(super) trait CatalogCredit: CatalogIds + Clone {
    /// The credit going by its MusicBrainz id alone.
    fn by_musicbrainz_alone(&self) -> Self;
}

/// What one write has decided before the credit it resolves now: the new
/// rows it makes, and the library rows whose ids it fills in. The catalog-id
/// step sees the library as these leave it.
pub(super) struct WriteSoFar<'a, R> {
    pub(super) inserts: &'a [R],
    pub(super) staged: &'a [R],
}

impl<R> Clone for WriteSoFar<'_, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R> Copy for WriteSoFar<'_, R> {}

/// What the catalog-id step says about one credit.
pub(super) enum CatalogMatch<R> {
    /// The credit carries no id a row holds.
    Unmatched,
    /// The row its ids name.
    Found(CatalogFound<R>),
    /// Its two ids disagree: they name two rows that are not one, or the row
    /// one names holds another id of the other catalog.
    Disagrees,
}

/// The row a credit's ids name, and a new row of this write that the credit
/// says is that one too.
pub(super) struct CatalogFound<R> {
    pub(super) holder: Holder<R>,
    /// A new row this write makes holding the credit's other id. It is not
    /// in the library yet, and no id it holds contradicts the credit or
    /// `holder`, so the credit makes it `holder`: it takes the place of every
    /// credit this write already resolved to it, and gives `holder` its ids.
    pub(super) folded: Option<R>,
}

/// The row one catalog id names, as this write leaves the library.
pub(super) enum Holder<R> {
    /// A library row.
    Library(Held<R>),
    /// A new row this write makes.
    New(R),
}

impl<R> Holder<R> {
    fn row(&self) -> &R {
        match self {
            Self::Library(held) => &held.row,
            Self::New(row) => row,
        }
    }
}

/// The library row a catalog id names, and the other library rows holding
/// it, which the write merges into it.
pub(super) struct Held<R> {
    pub(super) row: R,
    pub(super) absorbed: Vec<R>,
}

/// The catalog-id step: the row the credit's Discogs and MusicBrainz ids
/// name, as this write so far leaves the library.
pub(super) fn catalog_match_on<Q: coven::SqlRead, R: CatalogRow>(
    sql: &Q,
    credit: &impl CatalogIds,
    so_far: WriteSoFar<'_, R>,
) -> Result<CatalogMatch<R>, DbError> {
    let holder = |column| holder_on(sql, column, credit.catalog_id(column), so_far);
    let by_discogs = holder(CatalogColumn::Discogs)?;
    let by_musicbrainz = holder(CatalogColumn::MusicBrainz)?;
    let agrees = |holder: &Holder<R>| ids_agree(holder.row(), credit);
    Ok(match (by_discogs, by_musicbrainz) {
        (None, None) => CatalogMatch::Unmatched,
        (Some(one), None) | (None, Some(one)) => match agrees(&one) {
            true => CatalogMatch::Found(CatalogFound { holder: one, folded: None }),
            false => CatalogMatch::Disagrees,
        },
        (Some(Holder::Library(discogs)), Some(Holder::Library(musicbrainz)))
            if discogs.row.row_id() == musicbrainz.row.row_id() =>
        {
            let mut absorbed = discogs.absorbed;
            absorbed.extend(musicbrainz.absorbed);
            let holder = Holder::Library(Held { row: musicbrainz.row, absorbed });
            match agrees(&holder) {
                true => CatalogMatch::Found(CatalogFound { holder, folded: None }),
                false => CatalogMatch::Disagrees,
            }
        }
        (Some(Holder::New(discogs)), Some(Holder::New(musicbrainz)))
            if discogs.row_id() == musicbrainz.row_id() =>
        {
            let holder = Holder::New(musicbrainz);
            match agrees(&holder) {
                true => CatalogMatch::Found(CatalogFound { holder, folded: None }),
                false => CatalogMatch::Disagrees,
            }
        }
        // Two rows, one of them a new row of this write's: it folds into the
        // other where nothing either holds contradicts the credit or the
        // other. Two library rows stay two.
        (Some(Holder::New(new)), Some(holder)) | (Some(holder), Some(Holder::New(new)))
            if agrees(&holder) && ids_agree(&new, credit) && ids_agree(&new, holder.row()) =>
        {
            CatalogMatch::Found(CatalogFound { holder, folded: Some(new) })
        }
        (Some(_), Some(_)) => CatalogMatch::Disagrees,
    })
}

/// `credit`, of which the catalog-id step said `matched`, as the step settles
/// it, and the row its ids name. Where its two ids disagree, the MusicBrainz
/// id decides: the Discogs id is dropped from the credit, which then goes by
/// its MusicBrainz id alone, and nothing is given the Discogs id.
pub(super) fn settled_on<Q: coven::SqlRead, C: CatalogCredit, R: CatalogRow>(
    sql: &Q,
    credit: C,
    matched: CatalogMatch<R>,
    so_far: WriteSoFar<'_, R>,
) -> Result<(C, Option<CatalogFound<R>>), DbError> {
    match matched {
        CatalogMatch::Unmatched => Ok((credit, None)),
        CatalogMatch::Found(found) => Ok((credit, Some(found))),
        CatalogMatch::Disagrees => {
            let settled = credit.by_musicbrainz_alone();
            match catalog_match_on(sql, &settled, so_far)? {
                CatalogMatch::Unmatched => Ok((settled, None)),
                CatalogMatch::Found(found) => Ok((settled, Some(found))),
                CatalogMatch::Disagrees => {
                    unreachable!("a credit carrying one catalog id agrees with what it names")
                }
            }
        }
    }
}

/// The row holding the catalog id `id` of `column`, as this write so far
/// leaves the library: a new row it makes, else the library row
/// [`exact_holder`] names. No id is held by both, since a new row is made
/// only of ids no row holds.
fn holder_on<Q: coven::SqlRead, R: CatalogRow>(
    sql: &Q,
    column: CatalogColumn,
    id: Option<&str>,
    so_far: WriteSoFar<'_, R>,
) -> Result<Option<Holder<R>>, DbError> {
    let Some(id) = id else {
        return Ok(None);
    };
    if let Some(new) = so_far.inserts.iter().find(|new| new.catalog_id(column) == Some(id)) {
        return Ok(Some(Holder::New(new.clone())));
    }
    let stored = R::holding_on(sql, column, id)?;
    Ok(exact_holder(id, stored, so_far.staged, column).map(Holder::Library))
}

/// The one library row a credit's `column` id `catalog_id` names, as this
/// write's own fills (`staged`) leave it: of the rows the library holds it on
/// (`stored`) and those this write is giving it, the smallest id. One id is
/// one row, so the others are merged into it, and it takes the ids it lacks
/// from them. None is no match.
fn exact_holder<R: CatalogRow>(
    catalog_id: &str,
    stored: Vec<R>,
    staged: &[R],
    column: CatalogColumn,
) -> Option<Held<R>> {
    let staged_as = |id: &str| staged.iter().find(|staged| staged.row_id() == id);
    let mut named: Vec<R> =
        stored.into_iter().map(|row| staged_as(row.row_id()).cloned().unwrap_or(row)).collect();
    for row in staged {
        if row.catalog_id(column) == Some(catalog_id)
            && !named.iter().any(|named| named.row_id() == row.row_id())
        {
            named.push(row.clone());
        }
    }
    named.sort_by(|a, b| a.row_id().cmp(b.row_id()));
    let mut named = named.into_iter();
    let mut row = named.next()?;
    let absorbed: Vec<R> = named.collect();
    for other in &absorbed {
        row.fill_missing(other);
    }
    Some(Held { row, absorbed })
}

/// Give each new row of a write that a catalog names the id its catalog entry
/// derives from the ids the row holds once the write has decided every credit,
/// and name that id at each of the write's `places` naming the row by the id
/// it was made with. A row no catalog names keeps the fresh id it was made
/// with.
pub(super) fn identify_new_rows<'p, R: CatalogRow>(
    inserts: &mut [R],
    places: impl IntoIterator<Item = &'p mut String>,
) {
    let renamed: Vec<(String, String)> = inserts
        .iter_mut()
        .filter_map(|row| {
            let id = R::catalog_entry_id(
                row.catalog_id(CatalogColumn::MusicBrainz),
                row.catalog_id(CatalogColumn::Discogs),
            )?;
            let made_as = row.row_id().to_string();
            row.set_row_id(id.clone());
            Some((made_as, id))
        })
        .collect();
    for place in places {
        if let Some((_, id)) = renamed.iter().find(|(made_as, _)| made_as == place) {
            place.clone_from(id);
        }
    }
}
