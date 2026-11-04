//! Which library label each label of a release names — the one rule every
//! write of a release's labels decides by: an import committing its release,
//! and a metadata edit saving its pressing. Each runs it inside the
//! transaction that writes the release's labels, so it decides against the
//! library as committed at that moment.
//!
//! The rule, for each label of the release, in order:
//!
//! 1. **A label stating only a catalog number** names no library label.
//! 2. **An import's answer.** A label whose two catalog ids the person
//!    answered a conflict over names the label the answer says.
//! 3. **A catalog id is decisive.** A label carrying a MusicBrainz or Discogs
//!    id is the library label holding that id, followed through any merge.
//!    One id is one label: several library labels holding it, as two devices
//!    apart can leave them, are merged in the write, into the smallest id.
//!    Two different labels holding the label's two ids is the conflict a
//!    person settles on the import page; a matched label holding a different
//!    id of the same catalog is refused outright.
//! 4. **The label the release names.** On an edit, a label carrying no
//!    catalog id whose name is the name of a label the release names now is
//!    that label, so saving a release keeps its labels.
//! 5. **Otherwise the name.** A label is the one library label whose
//!    `name_key` equals the label's ([`bae_types::text_match::label_name_key`])
//!    and whose catalog ids do not contradict the label's. Several such
//!    labels, or none, make a new label.
//!
//! A label linked to an existing library label fills in that label's missing
//! catalog ids; the library label keeps its own id. A new label a catalog
//! names takes the id [`crate::identity::label_id`] derives from that
//! catalog entry, so every device that meets it writes one row; one known
//! only by name takes a fresh id.

use super::*;
use bae_types::pressing::ReleaseLabel;

/// What a write of a release's labels decides by, beside the labels.
pub struct LabelWrite<'a> {
    /// The ids a new label known only by name takes, in order: one for each
    /// label the release names, since each makes at most one.
    fresh_ids: &'a [String],
    /// `created_at` for a new label.
    now: &'a str,
    /// What a person answered for labels whose two catalog ids two library
    /// labels held, which an import's write follows.
    #[cfg(desktop)]
    answers: &'a [LabelAnswer],
}

/// A person's answer for a label whose Discogs and MusicBrainz ids two
/// library labels held: the label a place stating both ids names.
#[cfg(desktop)]
pub struct LabelAnswer {
    pub discogs_label_id: String,
    pub musicbrainz_label_id: String,
    pub label: AnsweredLabel,
}

/// The label a [`LabelAnswer`] names.
#[cfg(desktop)]
pub enum AnsweredLabel {
    /// A library label, by the id it shows as.
    Library(String),
    /// A new label carrying neither catalog id, made under this id.
    New(String),
}

/// The [`LabelWrite`] of a write that gives new labels the ids of `parts`,
/// made at its time.
pub(crate) fn label_write(parts: &LabelParts) -> LabelWrite<'_> {
    LabelWrite {
        fresh_ids: &parts.fresh_ids,
        now: &parts.now,
        #[cfg(desktop)]
        answers: &[],
    }
}

/// The `label_write` of an import, which follows `answers` to its label
/// conflicts.
#[cfg(desktop)]
pub fn import_label_write<'a>(parts: &'a LabelParts, answers: &'a [LabelAnswer]) -> LabelWrite<'a> {
    LabelWrite { answers, ..label_write(parts) }
}

/// What a write of labels takes along into its closure: an id for each label
/// that may be new, and the time the write makes them at.
pub struct LabelParts {
    fresh_ids: Vec<String>,
    now: String,
}

impl Database {
    /// The [`LabelParts`] of a write of `labels`.
    pub fn label_parts(&self, labels: &[ReleaseLabel]) -> LabelParts {
        LabelParts {
            fresh_ids: labels.iter().map(|_| self.inner.ids.new_id().to_string()).collect(),
            now: self.now_utc().to_rfc3339(),
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

    /// Whether no catalog id `label` carries contradicts this row's.
    fn agrees_with(&self, label: &ReleaseLabel) -> bool {
        crate::label_ids_are_compatible(
            &self.existing(),
            label.musicbrainz_label_id(),
            label.discogs_label_id(),
        )
    }

    /// Fill each catalog id this row lacks from `label`.
    fn fill_from(&mut self, label: &ReleaseLabel) {
        if self.musicbrainz_label_id.is_none() {
            self.musicbrainz_label_id = label.musicbrainz_label_id().map(str::to_string);
        }
        if self.discogs_label_id.is_none() {
            self.discogs_label_id = label.discogs_label_id().map(str::to_string);
        }
    }

    pub fn existing(&self) -> crate::ExistingLabel {
        crate::ExistingLabel {
            label_id: self.id.clone(),
            name: self.name.clone(),
            musicbrainz_label_id: self.musicbrainz_label_id.clone(),
            discogs_label_id: self.discogs_label_id.clone(),
        }
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

/// The labels one write decides: the new ones it inserts and the existing
/// ones whose catalog ids it fills in, as they will stand once it commits.
#[derive(Default)]
struct Decided {
    inserts: Vec<LabelRow>,
    fills: Vec<LabelRow>,
    /// How many of the write's fresh ids new labels have taken.
    fresh_taken: usize,
}

impl Decided {
    /// `label` as this write leaves it.
    fn overlaid(&self, label: LabelRow) -> LabelRow {
        self.fills.iter().find(|filled| filled.id == label.id).cloned().unwrap_or(label)
    }

    /// `merged`, a library label this write just merged others into, as this
    /// write leaves it: a fill this write already staged for it takes the
    /// catalog ids the merge gave it.
    fn merged(&mut self, merged: LabelRow) -> LabelRow {
        match self.fills.iter_mut().find(|filled| filled.id == merged.id) {
            Some(filled) => {
                if filled.musicbrainz_label_id.is_none() {
                    filled.musicbrainz_label_id = merged.musicbrainz_label_id;
                }
                if filled.discogs_label_id.is_none() {
                    filled.discogs_label_id = merged.discogs_label_id;
                }
                filled.clone()
            }
            None => merged,
        }
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
/// library label the rule in the module doc decides, inserting the new labels
/// and filling in the linked ones first.
pub fn write_release_labels_on(
    tx: &CovenWrite<'_>,
    release_id: &str,
    labels: &[ReleaseLabel],
    write: &LabelWrite<'_>,
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
    replace_release_label_rows(tx, release_id, &rows)?;
    Ok(())
}

/// The name and id of each label the release names now, as each shows.
fn labels_named_on<Q: QueryRows>(
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
    write: &LabelWrite<'_>,
) -> Result<String, IdentityWriteError> {
    #[cfg(desktop)]
    if let Some(id) = import_answer(label, name, decided, write) {
        return Ok(id);
    }

    let by_musicbrainz = label
        .musicbrainz_label_id()
        .map(|id| label_by_catalog_id_on(tx, CatalogColumn::MusicBrainz, id, decided, write.now))
        .transpose()?
        .flatten();
    let by_discogs = label
        .discogs_label_id()
        .map(|id| label_by_catalog_id_on(tx, CatalogColumn::Discogs, id, decided, write.now))
        .transpose()?
        .flatten();
    let matched = match (by_discogs, by_musicbrainz) {
        (Some(discogs), Some(musicbrainz)) if discogs.id != musicbrainz.id => {
            let pending = |row: &LabelRow| decided.inserts.iter().any(|new| new.id == row.id);
            match (pending(&discogs), pending(&musicbrainz)) {
                // A label this write makes stands for the library label the
                // other id names: every place that named it names that one.
                (true, false) => Some(absorb_pending(decided, rows, discogs, musicbrainz)),
                (false, true) => Some(absorb_pending(decided, rows, musicbrainz, discogs)),
                _ => {
                    if !discogs.agrees_with(label) || !musicbrainz.agrees_with(label) {
                        return Err(contradicted(name));
                    }
                    let (Some(discogs_label_id), Some(musicbrainz_label_id)) = (
                        label.discogs_label_id().map(str::to_string),
                        label.musicbrainz_label_id().map(str::to_string),
                    ) else {
                        unreachable!("two catalog matches come from the label's two catalog ids")
                    };
                    return Err(IdentityWriteError::LabelIdentityConflict(Box::new(
                        crate::LabelIdentityConflict {
                            incoming_label_name: name.to_string(),
                            discogs_label_id,
                            musicbrainz_label_id,
                            discogs_label: discogs.existing(),
                            musicbrainz_label: musicbrainz.existing(),
                        },
                    )));
                }
            }
        }
        (Some(found), _) | (_, Some(found)) => Some(found),
        (None, None) => None,
    };
    if let Some(existing) = matched {
        if !existing.agrees_with(label) {
            return Err(contradicted(name));
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
            .filter(|pending| same_key(pending) && pending.agrees_with(label))
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
                .map(|row| decided.overlaid(row))
                .filter(|row| row.agrees_with(label))
                .collect();
            if let [only] = named.as_slice() {
                return Ok(decided.link(only.clone(), label));
            }
        }
    }

    let id = match crate::identity::label_id(label.musicbrainz_label_id(), label.discogs_label_id())
    {
        Some(id) => id,
        None => {
            let id = write.fresh_ids.get(decided.fresh_taken).cloned().ok_or_else(|| {
                IdentityWriteError::Unresolvable(format!(
                    "label '{name}' is one more new label than the write has ids for"
                ))
            })?;
            decided.fresh_taken += 1;
            id
        }
    };
    decided.inserts.push(LabelRow {
        id: id.clone(),
        name: name.to_string(),
        musicbrainz_label_id: label.musicbrainz_label_id().map(str::to_string),
        discogs_label_id: label.discogs_label_id().map(str::to_string),
    });
    Ok(id)
}

/// The library label the write's answer to a conflict over `label`'s two
/// catalog ids names, made new where the answer was neither; `None` where it
/// holds no answer for these ids.
#[cfg(desktop)]
fn import_answer(
    label: &ReleaseLabel,
    name: &str,
    decided: &mut Decided,
    write: &LabelWrite<'_>,
) -> Option<String> {
    let (Some(musicbrainz), Some(discogs)) =
        (label.musicbrainz_label_id(), label.discogs_label_id())
    else {
        return None;
    };
    let answer = write.answers.iter().find(|answer| {
        answer.discogs_label_id == discogs && answer.musicbrainz_label_id == musicbrainz
    });
    match answer.map(|answer| &answer.label) {
        Some(AnsweredLabel::Library(id)) => Some(id.clone()),
        Some(AnsweredLabel::New(id)) => {
            let id = id.clone();
            if !decided.inserts.iter().any(|pending| pending.id == id) {
                decided.inserts.push(LabelRow {
                    id: id.clone(),
                    name: name.to_string(),
                    musicbrainz_label_id: None,
                    discogs_label_id: None,
                });
            }
            Some(id)
        }
        None => None,
    }
}

fn contradicted(name: &str) -> IdentityWriteError {
    IdentityWriteError::Unresolvable(format!(
        "label '{name}' has catalog ids that contradict the library label they match"
    ))
}

/// Drop the label `pending` this write was making in favour of the library
/// label `existing`, which takes its catalog ids; every place already decided
/// as `pending` names `existing`.
fn absorb_pending(
    decided: &mut Decided,
    rows: &mut [(Option<String>, Option<&str>)],
    pending: LabelRow,
    existing: LabelRow,
) -> LabelRow {
    decided.inserts.retain(|new| new.id != pending.id);
    for (id, _) in rows.iter_mut() {
        if id.as_deref() == Some(pending.id.as_str()) {
            *id = Some(existing.id.clone());
        }
    }
    let carried = ReleaseLabel::new(Some(pending.name), None)
        .expect("a label this write makes has a name")
        .with_ids(pending.musicbrainz_label_id, pending.discogs_label_id);
    let existing = decided.overlaid(existing);
    decided.link(existing.clone(), &carried);
    decided.overlaid(existing)
}

#[derive(Clone, Copy)]
enum CatalogColumn {
    Discogs,
    MusicBrainz,
}

/// The label holding `catalog_id`: one this write makes, else the library
/// label whose `column` holds it or whose row the catalog id names (its
/// column may have been filled in or edited since), followed through any
/// merge to the label it shows as, as this write leaves it. One catalog id is
/// one label, so several library labels holding it are merged here, at
/// `now`, into the smallest id, which takes the ids it lacks from the others.
fn label_by_catalog_id_on(
    tx: &CovenWrite<'_>,
    column: CatalogColumn,
    catalog_id: &str,
    decided: &mut Decided,
    now: &str,
) -> Result<Option<LabelRow>, IdentityWriteError> {
    let held = |row: &LabelRow| match column {
        CatalogColumn::Discogs => row.discogs_label_id.as_deref() == Some(catalog_id),
        CatalogColumn::MusicBrainz => row.musicbrainz_label_id.as_deref() == Some(catalog_id),
    };
    if let Some(pending) = decided.inserts.iter().find(|pending| held(pending)) {
        return Ok(Some(pending.clone()));
    }
    if let Some(filled) = decided.fills.iter().find(|filled| held(filled)) {
        return Ok(Some(filled.clone()));
    }
    let (column, identity) = match column {
        CatalogColumn::Discogs => {
            ("discogs_label_id", crate::identity::label_id(None, Some(catalog_id)))
        }
        CatalogColumn::MusicBrainz => {
            ("musicbrainz_label_id", crate::identity::label_id(Some(catalog_id), None))
        }
    };
    let mut holding: Vec<LabelRow> = tx.query(
        &format!(
            "SELECT l.* FROM labels l WHERE l.id IN ( \
                 SELECT {} FROM labels named \
                 WHERE named.{column} = ?1 OR named.id = ?2) \
             ORDER BY l.id",
            shown_label_id("named.id")
        ),
        params![catalog_id, identity],
        LabelRow::from_row,
    )?;
    if holding.len() < 2 {
        return Ok(holding.pop().map(|row| decided.overlaid(row)));
    }
    let survivor = holding.remove(0).id;
    for absorbed in holding {
        let absorbed = decided.overlaid(absorbed);
        super::identity_merges::merge_label_on(
            tx,
            &super::identity_merges::LabelMerge {
                survivor: &survivor,
                absorbed: &absorbed.id,
                discogs_label_id: absorbed.discogs_label_id.as_deref(),
                musicbrainz_label_id: absorbed.musicbrainz_label_id.as_deref(),
                at: now,
            },
        )?;
    }
    let merged =
        tx.query_row("SELECT * FROM labels WHERE id = ?", [&survivor], LabelRow::from_row)?;
    Ok(Some(decided.merged(merged)))
}

/// Make the release `release_id`'s label rows exactly `rows`, in order,
/// touching only the rows that differ: a row is a place in the list, so
/// another device's edit of another place merges with this one.
fn replace_release_label_rows(
    tx: &CovenWrite<'_>,
    release_id: &str,
    rows: &[(Option<String>, Option<&str>)],
) -> Result<(), DbError> {
    let stored: HashMap<String, (Option<String>, Option<String>)> = tx
        .query(
            "SELECT id, label_id, catalog_number FROM release_labels WHERE release_id = ?",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?))),
        )?
        .into_iter()
        .collect();
    let ids: Vec<String> = (0..rows.len())
        .map(|position| crate::identity::release_label_id(release_id, position))
        .collect();
    for id in stored.keys() {
        if !ids.contains(id) {
            tx.execute("DELETE FROM release_labels WHERE id = ?", params![id])?;
        }
    }
    for (position, (id, (label_id, catalog_number))) in ids.iter().zip(rows).enumerate() {
        let wanted = (label_id.clone(), catalog_number.map(str::to_string));
        match stored.get(id) {
            Some(stored) if *stored == wanted => {}
            Some(_) => {
                tx.execute(
                    "UPDATE release_labels SET label_id = ?, catalog_number = ? WHERE id = ?",
                    params![label_id, catalog_number, id],
                )?;
            }
            None => {
                tx.execute(
                    "INSERT INTO release_labels \
                         (id, release_id, position, label_id, catalog_number) \
                     VALUES (?, ?, ?, ?, ?)",
                    params![id, release_id, position as i64, label_id, catalog_number],
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "label_resolution_tests.rs"]
mod tests;
