//! Which library label each label of a release names — the one rule every
//! write of a release's labels decides by: an import committing its release,
//! and a metadata edit saving its pressing. Each runs it inside the
//! transaction that writes the release's labels, so it decides against the
//! library as committed at that moment.
//!
//! The rule, for each label of the release, in order:
//!
//! 1. **A label stating only a catalog number** names no library label.
//! 2. **A catalog id is decisive.** A label carrying a MusicBrainz or Discogs
//!    id is the label holding that id, as the write so far leaves the
//!    library: a library label followed through any merge, or a new label an
//!    earlier label of the same write makes. This is the step artist credits
//!    take (`catalog_match`): several library labels holding one id are
//!    merged in the write, into the smallest id; where the label's two ids
//!    disagree, the MusicBrainz id decides and the Discogs id is dropped from
//!    the label and given to nothing; where they name a new label of the
//!    write's and another label, and no id either holds contradicts the
//!    label or the other, the new one is the other.
//! 3. **The label the release names.** On an edit, a label carrying no
//!    catalog id whose name is the name of a label the release names now is
//!    that label, so saving a release keeps its labels.
//! 4. **Otherwise the name.** A label is the one library label whose
//!    `name_key` equals the label's ([`bae_types::text_match::label_name_key`])
//!    and whose catalog ids do not contradict the label's. Several such
//!    labels, or none, make a new label.
//!
//! A label linked to an existing library label fills in that label's missing
//! catalog ids; the library label keeps its own id. A new label a catalog
//! names takes the id [`crate::identity::label_id`] derives from that
//! catalog entry, from the ids it holds once the write has decided every
//! label (`catalog_match`): one catalog entry is one row; one known only by
//! name keeps the fresh id it was made with.

use super::catalog_match::*;
use super::*;
use bae_types::pressing::ReleaseLabel;

/// What a write of a release's pressing decides by, beside the pressing: the
/// ids it makes rows with, and its time.
pub struct PressingWrite<'a> {
    /// The ids new labels are made with, in order: one for each label the
    /// release names, since each makes at most one. A new label a catalog
    /// names takes its catalog entry's id once the write has decided every
    /// label.
    fresh_ids: &'a [String],
    /// The ids of the release's label rows, one for each label.
    label_row_ids: &'a [String],
    /// The ids of the release's barcode rows, one for each barcode.
    pub(super) barcode_ids: &'a [String],
    /// The ids of the release's media rows, one for each carrier.
    pub(super) media_ids: &'a [String],
    /// `created_at` for a new label.
    now: &'a str,
}

/// The [`PressingWrite`] of a write that makes rows with the ids of `parts`,
/// made at its time.
pub fn pressing_write(parts: &PressingParts) -> PressingWrite<'_> {
    PressingWrite {
        fresh_ids: &parts.fresh_ids,
        label_row_ids: &parts.label_row_ids,
        barcode_ids: &parts.barcode_ids,
        media_ids: &parts.media_ids,
        now: &parts.now,
    }
}

/// What a write of a pressing takes along into its closure: an id for each
/// label that may be new and for each row of its lists, and the time the
/// write makes them at.
pub struct PressingParts {
    fresh_ids: Vec<String>,
    label_row_ids: Vec<String>,
    barcode_ids: Vec<String>,
    media_ids: Vec<String>,
    now: String,
}

impl PressingParts {
    /// The parts of a write of `pressing` at `now`: an id from `ids` for each
    /// label that may be new and each row of its labels, barcodes and media.
    pub fn new(
        ids: &dyn coven::IdSource,
        now: chrono::DateTime<chrono::Utc>,
        pressing: &bae_types::pressing::Pressing,
    ) -> Self {
        let mint = |count: usize| (0..count).map(|_| ids.new_id().to_string()).collect::<Vec<_>>();
        Self {
            fresh_ids: mint(pressing.labels.len()),
            label_row_ids: mint(pressing.labels.len()),
            barcode_ids: mint(pressing.barcodes.len()),
            media_ids: mint(pressing.facts.media.len()),
            now: now.to_rfc3339(),
        }
    }
}

/// One `labels` row as resolution reads and writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRow {
    pub id: String,
    pub name: String,
    pub musicbrainz_label_id: Option<String>,
    pub discogs_label_id: Option<String>,
}

impl LabelRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            musicbrainz_label_id: row.get("musicbrainz_label_id")?,
            discogs_label_id: row.get("discogs_label_id")?,
        })
    }

    /// Fill each catalog id this row lacks from `label`, a row or a
    /// release's label of the same label.
    fn fill_from(&mut self, label: &impl CatalogIds) {
        let id = |column| label.catalog_id(column).map(str::to_string);
        if self.musicbrainz_label_id.is_none() {
            self.musicbrainz_label_id = id(CatalogColumn::MusicBrainz);
        }
        if self.discogs_label_id.is_none() {
            self.discogs_label_id = id(CatalogColumn::Discogs);
        }
    }
}

impl CatalogIds for LabelRow {
    fn catalog_id(&self, column: CatalogColumn) -> Option<&str> {
        match column {
            CatalogColumn::Discogs => self.discogs_label_id.as_deref(),
            CatalogColumn::MusicBrainz => self.musicbrainz_label_id.as_deref(),
        }
    }
}

impl CatalogRow for LabelRow {
    fn row_id(&self) -> &str {
        &self.id
    }

    fn set_row_id(&mut self, id: String) {
        self.id = id;
    }

    fn catalog_entry_id(musicbrainz: Option<&str>, discogs: Option<&str>) -> Option<String> {
        crate::identity::label_id(musicbrainz, discogs)
    }

    fn fill_missing(&mut self, other: &Self) {
        self.fill_from(other);
    }

    fn holding_on<Q: coven::SqlRead>(
        sql: &Q,
        column: CatalogColumn,
        catalog_id: &str,
    ) -> Result<Vec<Self>, DbError> {
        let (column, identity) = match column {
            CatalogColumn::Discogs => {
                ("discogs_label_id", Self::catalog_entry_id(None, Some(catalog_id)))
            }
            CatalogColumn::MusicBrainz => {
                ("musicbrainz_label_id", Self::catalog_entry_id(Some(catalog_id), None))
            }
        };
        Ok(sql.query(
            &format!(
                "SELECT l.* FROM labels l WHERE l.id IN ( \
                     SELECT {} FROM labels named \
                     WHERE named.{column} = ?1 OR named.id = ?2) \
                 ORDER BY l.id",
                shown_label_id("named.id")
            ),
            params![catalog_id, identity],
            LabelRow::from_row,
        )?)
    }
}

impl CatalogIds for ReleaseLabel {
    fn catalog_id(&self, column: CatalogColumn) -> Option<&str> {
        match column {
            CatalogColumn::Discogs => self.discogs_label_id(),
            CatalogColumn::MusicBrainz => self.musicbrainz_label_id(),
        }
    }
}

impl CatalogCredit for ReleaseLabel {
    fn by_musicbrainz_alone(&self) -> Self {
        self.clone().with_ids(self.musicbrainz_label_id().map(str::to_string), None)
    }
}

/// The SQL of the label `label_id` shows as: the end of its merge chain, else
/// itself.
pub fn shown_label_id(label_id: &str) -> String {
    format!(
        "COALESCE((SELECT survivor_id FROM merged_label_survivors \
                   WHERE label_id = {label_id}), {label_id})"
    )
}

/// The labels one write decides: the new ones it inserts, the existing ones
/// whose catalog ids it fills in, as they will stand once it commits, and
/// the library labels it merges into another.
#[derive(Default)]
struct Decided {
    inserts: Vec<LabelRow>,
    fills: Vec<LabelRow>,
    merges: Vec<LabelIdMerge>,
    /// How many of the write's fresh ids new labels have taken.
    fresh_taken: usize,
}

/// A library label this write merges into another, `survivor`, because both
/// hold one catalog id.
struct LabelIdMerge {
    survivor: String,
    /// The absorbed label as this write leaves it, whose ids the survivor
    /// takes where it has none.
    absorbed: LabelRow,
}

impl Decided {
    /// `label` as this write leaves it.
    fn overlaid(&self, label: LabelRow) -> LabelRow {
        self.fills.iter().find(|filled| filled.id == label.id).cloned().unwrap_or(label)
    }

    /// Record that `absorbed` is merged into `survivor`, once, and name the
    /// survivor wherever this write already placed `absorbed`.
    fn merge(
        &mut self,
        rows: &mut [(Option<String>, Option<&str>)],
        survivor: &str,
        absorbed: LabelRow,
    ) {
        name_instead(rows, &absorbed.id, survivor);
        if !self.merges.iter().any(|merge| merge.absorbed.id == absorbed.id) {
            self.merges.push(LabelIdMerge { survivor: survivor.to_string(), absorbed });
        }
    }

    /// Drop `folded`, a label this write was making, for `into`: every place
    /// already decided as `folded` names `into`.
    fn fold(&mut self, rows: &mut [(Option<String>, Option<&str>)], folded: &LabelRow, into: &str) {
        self.inserts.retain(|new| new.id != folded.id);
        name_instead(rows, &folded.id, into);
    }

    /// Link `label` to `existing`, filling in its catalog ids, and say which
    /// it is.
    fn link(&mut self, mut existing: LabelRow, label: &ReleaseLabel) -> String {
        let id = existing.id.clone();
        if let Some(pending) = self.inserts.iter_mut().find(|pending| pending.id == id) {
            pending.fill_from(label);
            return id;
        }
        let before = existing.clone();
        existing.fill_from(label);
        if existing != before {
            match self.fills.iter_mut().find(|filled| filled.id == id) {
                Some(filled) => *filled = existing,
                None => self.fills.push(existing),
            }
        }
        id
    }
}

/// Make the release `release_id`'s labels `labels`, in order, each naming the
/// library label the rule in the module doc decides, inserting the new labels,
/// filling in the linked ones and merging the labels one id names first.
pub fn write_release_labels_on(
    tx: &CovenWrite<'_>,
    release_id: &str,
    labels: &[ReleaseLabel],
    write: &PressingWrite<'_>,
) -> Result<(), IdentityWriteError> {
    let named_now = labels_named_on(tx, release_id)?;
    let mut decided = Decided::default();
    let mut rows: Vec<(Option<String>, Option<&str>)> = Vec::with_capacity(labels.len());
    for label in labels {
        let id = match label.name() {
            None => None,
            Some(name) => {
                Some(resolve_label_on(tx, label, name, &named_now, &mut decided, &mut rows, write)?)
            }
        };
        rows.push((id, label.catalog_number()));
    }
    identify_new_rows(&mut decided.inserts, rows.iter_mut().filter_map(|(id, _)| id.as_mut()));
    let key = |name: &str| bae_types::text_match::label_name_key(name);
    for label in &decided.inserts {
        tx.execute(
            "INSERT INTO labels \
                 (id, name, name_key, musicbrainz_label_id, discogs_label_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                label.id,
                label.name,
                key(&label.name),
                label.musicbrainz_label_id,
                label.discogs_label_id,
                write.now
            ],
        )?;
    }
    for label in &decided.fills {
        tx.execute(
            "UPDATE labels SET musicbrainz_label_id = ?, discogs_label_id = ? WHERE id = ?",
            params![label.musicbrainz_label_id, label.discogs_label_id, label.id],
        )?;
    }
    for merge in &decided.merges {
        super::identity_merges::merge_label_on(
            tx,
            &super::identity_merges::LabelMerge {
                survivor: &merge.survivor,
                absorbed: &merge.absorbed.id,
                discogs_label_id: merge.absorbed.discogs_label_id.as_deref(),
                musicbrainz_label_id: merge.absorbed.musicbrainz_label_id.as_deref(),
                at: write.now,
            },
        )?;
    }
    replace_release_label_rows(tx, release_id, &rows, write.label_row_ids)?;
    Ok(())
}

/// The name and id of each label the release names now, as each shows.
fn labels_named_on<Q: coven::SqlRead>(
    sql: &Q,
    release_id: &str,
) -> Result<Vec<(String, String)>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT shown.name, shown.id FROM release_labels named \
             JOIN labels shown ON shown.id = {} \
             WHERE named.release_id = ? ORDER BY named.position",
            shown_label_id("named.label_id")
        ),
        [release_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

/// The library label `label`, named `name`, names.
fn resolve_label_on(
    tx: &CovenWrite<'_>,
    label: &ReleaseLabel,
    name: &str,
    named_now: &[(String, String)],
    decided: &mut Decided,
    rows: &mut [(Option<String>, Option<&str>)],
    write: &PressingWrite<'_>,
) -> Result<String, IdentityWriteError> {
    let so_far = WriteSoFar { inserts: &decided.inserts, staged: &decided.fills };
    let matched = catalog_match_on(tx, label, so_far)?;
    let (label, found) = settled_on(tx, label.clone(), matched, so_far)?;
    let label = &label;
    if let Some(CatalogFound { holder, folded }) = found {
        let existing = match holder {
            Holder::Library(held) => {
                for absorbed in held.absorbed {
                    decided.merge(rows, &held.row.id, absorbed);
                }
                held.row
            }
            Holder::New(new) => new,
        };
        // The folded label's ids are the label's own: it holds one of them
        // and no id of the other catalog, or both would name it.
        if let Some(folded) = folded {
            decided.fold(rows, &folded, &existing.id);
        }
        return Ok(decided.link(existing, label));
    }

    let carries_ids = label.musicbrainz_label_id().is_some() || label.discogs_label_id().is_some();
    if !carries_ids {
        if let Some((_, id)) = named_now.iter().find(|(shown, _)| shown == name) {
            return Ok(id.clone());
        }
    }

    let key = bae_types::text_match::label_name_key(name);
    if !key.is_empty() {
        let same_key = |row: &LabelRow| bae_types::text_match::label_name_key(&row.name) == key;
        let pending: Vec<LabelRow> = decided
            .inserts
            .iter()
            .filter(|pending| same_key(pending) && ids_agree(*pending, label))
            .cloned()
            .collect();
        if let [only] = pending.as_slice() {
            return Ok(decided.link(only.clone(), label));
        }
        if pending.is_empty() {
            let named: Vec<LabelRow> = tx
                .query(
                    "SELECT * FROM labels WHERE name_key = ? \
                       AND id NOT IN (SELECT label_id FROM merged_label_survivors) \
                     ORDER BY id",
                    [&key],
                    LabelRow::from_row,
                )?
                .into_iter()
                // A label this write merges into another is that one.
                .filter(|row| !decided.merges.iter().any(|merge| merge.absorbed.id == row.id))
                .map(|row| decided.overlaid(row))
                .filter(|row| ids_agree(row, label))
                .collect();
            if let [only] = named.as_slice() {
                return Ok(decided.link(only.clone(), label));
            }
        }
    }

    let id = write
        .fresh_ids
        .get(decided.fresh_taken)
        .cloned()
        .expect("a write holds a fresh id for each label it names");
    decided.fresh_taken += 1;
    decided.inserts.push(LabelRow {
        id: id.clone(),
        name: name.to_string(),
        musicbrainz_label_id: label.musicbrainz_label_id().map(str::to_string),
        discogs_label_id: label.discogs_label_id().map(str::to_string),
    });
    Ok(id)
}

/// Make every place of `rows` naming the label `from` name `to`.
fn name_instead(rows: &mut [(Option<String>, Option<&str>)], from: &str, to: &str) {
    for (id, _) in rows.iter_mut() {
        if id.as_deref() == Some(from) {
            *id = Some(to.to_string());
        }
    }
}

/// Make the release `release_id`'s label rows exactly `rows`, in order,
/// each under its id of `row_ids`.
fn replace_release_label_rows(
    tx: &CovenWrite<'_>,
    release_id: &str,
    rows: &[(Option<String>, Option<&str>)],
    row_ids: &[String],
) -> Result<(), DbError> {
    assert_eq!(rows.len(), row_ids.len(), "a write holds an id for each label row it makes");
    tx.execute("DELETE FROM release_labels WHERE release_id = ?", params![release_id])?;
    for (position, ((label_id, catalog_number), id)) in rows.iter().zip(row_ids).enumerate() {
        tx.execute(
            "INSERT INTO release_labels (id, release_id, position, label_id, catalog_number) \
             VALUES (?, ?, ?, ?, ?)",
            params![id, release_id, position as i64, label_id, catalog_number],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "label_resolution_tests.rs"]
mod tests;
