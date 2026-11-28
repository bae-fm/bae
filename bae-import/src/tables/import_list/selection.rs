//! A list's selection, which the surface keeps as the keys of the rows the
//! person selected: what those rows hold and offer, read as a live query of
//! its own whatever the list shows; the ones the list shows, in its order,
//! which a bulk action takes; the keys a Shift run and Select All select; and
//! which selected rows another view or a fold takes out of the list.

use super::list_items::{order_name, Item, ListBinding};
use super::*;
use crate::selection::{SelectedCandidate, SelectedRows, SelectionSummary, SingleSelection};
use crate::triage::SelectionOffer;
use crate::{CandidateAction, CandidateKey, ImportListOpening, ImportListViewChange};

// What the releases at `keys`, a JSON array of their keys in hex, hold and
// offer: how many rows the keys name, the one row when they name one, and a
// row for each command the selection offers, as its place in `commands`, a
// JSON array of the flags `ListedActions` names them by, with how many rows
// it runs over and whether it can run. A command runs row by row over the rows
// that offer it, so the selection offers it when any row does. Combine is one
// command over the whole selection: the selection offers it for two or more
// rows and enables it, counting every row, only when every row can join one
// release; otherwise it counts none. A selection offering nothing, such as one
// whose keys name no row, gives one row with no command.
coven::read!(
    writer Selection(keys: String, commands: String) {
        rows: i64, single_item: Option<String>, single_key: Option<crate::CandidateKey>,
        command: Option<i64>, offering: Option<i64>, enabled: Option<bool>
    },
    r#"
    WITH chosen(item, key, actions) AS (
        SELECT p.item, p.key,
            @crate::tables::import_list::row_fragments::ListedActions(parent: p)
        FROM json_each(:keys) AS asked
        JOIN import_list_placement AS p ON p.key = unhex(asked.value) AND p.item <> 'group'
    ), total(rows, item, key) AS (
        SELECT count(*), MIN(chosen.item), MIN(chosen.key) FROM chosen
    ), held(flag, offering, every_row) AS (
        SELECT flag.key, SUM(flag.value), MIN(flag.value)
        FROM chosen, json_each(chosen.actions) AS flag
        GROUP BY flag.key
    ), offered(command, offering, enabled) AS (
        SELECT named.key,
            CASE WHEN held.flag <> 'can_Combine' THEN held.offering
                 WHEN held.every_row THEN total.rows ELSE 0 END,
            held.flag <> 'can_Combine' OR held.every_row
        FROM json_each(:commands) AS named
        JOIN held ON held.flag = named.value
        CROSS JOIN total
        WHERE CASE WHEN held.flag = 'can_Combine' THEN total.rows >= 2
                   ELSE held.offering > 0 END
    )
    SELECT total.rows, iif(total.rows = 1, total.item, NULL) AS single_item,
        iif(total.rows = 1, total.key, NULL) AS single_key,
        offered.command, offered.offering, offered.enabled
    FROM total
    LEFT JOIN offered ON 1
    ORDER BY offered.command
"#
);

/// What the rows at `keys` hold and offer, in one statement, whatever the
/// list shows: how many rows the keys name, the one row when they name one,
/// and the commands the selection offers, in the order of
/// [`CandidateAction::ALL`]. A key whose row the tables no longer hold names
/// no row. A row that Combine cannot merge shows the reason, and Combine
/// stays in the offers.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn selection(
    sql: &impl bae_store::WriterRead,
    keys: &[CandidateKey],
) -> Result<SelectionSummary, DbError> {
    selection_of(statement_rows(sql, &selection_params(keys))?)
}

/// What the selected rows hold and offer, as their read returns it.
pub(super) fn selection_of(rows: Vec<SelectionRow>) -> Result<SelectionSummary, DbError> {
    let mut summary = SelectionSummary::default();
    for row in rows {
        summary.count = to_u64(row.rows, "a selection's count")?;
        let item = row.single_item.as_deref().map(Item::of).transpose()?;
        summary.single = match (item, row.single_key) {
            (Some(Item::Candidate), Some(key)) => Some(SingleSelection::Candidate { key }),
            (Some(Item::Invalid), Some(key)) => Some(SingleSelection::InvalidCandidate { key }),
            _ => None,
        };
        if let (Some(command), Some(offering), Some(enabled)) =
            (row.command, row.offering, row.enabled)
        {
            let action = usize::try_from(command)
                .ok()
                .and_then(|command| CandidateAction::ALL.get(command).copied())
                .ok_or_else(|| DbError::unreadable("a selection's command", command))?;
            let count = to_u64(offering, "a selection's count")?;
            summary.offers.push(SelectionOffer { action, count, enabled });
        }
    }
    Ok(summary)
}

/// The statement reading what the rows at `keys` hold and offer.
pub(super) fn selection_params(keys: &[CandidateKey]) -> SelectionParams {
    let commands: Vec<String> = CandidateAction::ALL.into_iter().map(action_flag).collect();
    SelectionParams {
        keys: crate::tables::key_hex_list(keys.iter()),
        commands: serde_json::to_string(&commands).expect("a list of text serializes"),
    }
}

// The releases among `keys`, a JSON array of their keys in hex, that the list
// shows, in its order, each with its item and its folder's name; and, when
// `acting` names a command's flag, only those that offer the command.
coven::read!(
    writer Selected(
        list_order: String, tab: Option<String>, filter: Option<String>,
        filter_text: Option<String>, folds_only: bool, folded: String,
        opened: Option<crate::CandidateKey>, keys: String, acting: Option<String>
    ) { name: Option<String> },
    r#"
    WITH listed(release_key, release_position) AS (
        SELECT shown.release_key, shown.release_position
        FROM @crate::tables::import_list::list_items::ShownReleases(
            list_order: :list_order, tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS shown
    ), named(root, key, name) AS (
        SELECT scan.listed_root, scan.listed_key, scan.listed_name
        FROM @crate::tables::folder_scans::listed_fragments::ListedScan AS scan
    )
    SELECT p.item, p.key, named.name
    FROM json_each(:keys) AS asked
    JOIN listed ON listed.release_key = unhex(asked.value)
    JOIN import_list_placement AS p ON p.key = listed.release_key AND p.item <> 'group'
    JOIN named ON named.root = p.watched_folder_path AND named.key = p.key
    WHERE :acting IS NULL
       OR (p.item = 'candidate' AND json_extract(
               @crate::tables::import_list::row_fragments::ListedActions(parent: p),
               '$.' || :acting) = 1)
    ORDER BY listed.release_position
"#
);

// Every release the list shows, in its order.
coven::read!(
    writer ShownKeys(
        list_order: String,
        tab: Option<String>,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        opened: Option<crate::CandidateKey>,
    ),
    r#"
    WITH listed(release_key, release_position) AS (
        SELECT shown.release_key, shown.release_position
        FROM @crate::tables::import_list::list_items::ShownReleases(
        list_order: :list_order, tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS shown
    )
    SELECT listed.release_key AS key
    FROM listed
    ORDER BY listed.release_position
"#
);

// The releases the list shows from the release `first` to the release
// `last`, both included, in the list's order: those whose places are between
// the two ends' places, which are one place when the list shows one end alone.
coven::read!(
    writer KeysBetween(
        list_order: String,
        tab: Option<String>,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        opened: Option<crate::CandidateKey>,
        first: crate::CandidateKey,
        last: crate::CandidateKey,
    ),
    r#"
    WITH listed(release_key, release_position) AS (
        SELECT shown.release_key, shown.release_position
        FROM @crate::tables::import_list::list_items::ShownReleases(
            list_order: :list_order, tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS shown
    ), ends(low, high) AS (
        SELECT MIN(listed.release_position), MAX(listed.release_position)
        FROM listed
        WHERE listed.release_key IN (:first, :last)
    )
    SELECT listed.release_key AS key
    FROM listed
    CROSS JOIN ends
    WHERE listed.release_position BETWEEN ends.low AND ends.high
    ORDER BY listed.release_position
"#
);

// Where the release at `key` sits in the list's order; no row when the list
// does not show it.
coven::read!(
    writer ShownPosition(
        list_order: String, tab: Option<String>, filter: Option<String>,
        filter_text: Option<String>, folds_only: bool, folded: String,
        opened: Option<crate::CandidateKey>, key: crate::CandidateKey
    ) { position: i64 },
    r#"
    WITH listed(release_key, release_position) AS (
        SELECT shown.release_key, shown.release_position
        FROM @crate::tables::import_list::list_items::ShownReleases(
            list_order: :list_order, tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS shown
    )
    SELECT listed.release_position AS position
    FROM listed
    WHERE listed.release_key = :key
"#
);

// Where a list opens for `keys`, a JSON array of release keys in hex: the
// place of the first of them the list shows, none when it shows none; and
// those it doesn't show, in their order.
coven::read!(
    writer Opening(
        list_order: String, tab: Option<String>, filter: Option<String>,
        filter_text: Option<String>, folds_only: bool, folded: String,
        opened: Option<crate::CandidateKey>, keys: String
    ) {
        struct Left { key: hex }
        first_position: Option<i64>, left_keys: json<Vec<Left>>
    },
    r#"
    WITH listed(release_key, release_position) AS (
        SELECT shown.release_key, shown.release_position
        FROM @crate::tables::import_list::list_items::ShownReleases(
            list_order: :list_order, tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS shown
    ), asked(position, key, at) AS (
        SELECT listed.release_position, unhex(named.value), named.key
        FROM json_each(:keys) AS named
        LEFT JOIN listed ON listed.release_key = unhex(named.value)
    )
    SELECT (SELECT MIN(asked.position) FROM asked) AS first_position,
        (SELECT json_group_array(json_object('key', hex(asked.key)) ORDER BY asked.at)
         FROM asked
         WHERE asked.position IS NULL) AS left_keys
"#
);

// The releases among `keys`, a JSON array of their keys in hex, that a fold
// hid: in the view's tab and kept by its filters, shown under the folds
// `folded_before` and in a group the folds `folded` fold shut.
coven::read!(
    writer HiddenByFolds(
        tab: String,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only_before: bool,
        folded_before: String,
        folds_only: bool,
        folded: String,
        keys: String,
    ),
    r#"
    SELECT p.key
    FROM json_each(:keys) AS asked
    JOIN import_list_placement AS p ON p.key = unhex(asked.value) AND p.item <> 'group'
    WHERE p.tab = :tab
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: p)
      AND p.group_folder IS NOT NULL
      AND @crate::tables::import_list::row_fragments::Folded(
          folds_only: :folds_only, folded: :folded, parent: p)
      AND NOT @crate::tables::import_list::row_fragments::Folded(
          folds_only: :folds_only_before, folded: :folded_before, parent: p)
"#
);

/// The flag `ListedActions` names `action` by.
fn action_flag(action: CandidateAction) -> String {
    format!("can_{action:?}")
}

/// The parameters of a statement over the releases the list `request` shows:
/// those [`ShownReleases`] binds, and `rest`.
macro_rules! shown_params {
    ($params:ident, $request:expr $(, $field:ident : $value:expr)*) => {{
        let request: &ImportListRequest = $request;
        let ListBinding { tab, filter, filter_text, folds_only, folded } =
            ListBinding::of(request);
        $params {
            list_order: order_name(request.view.order),
            tab: Some(tab),
            filter,
            filter_text,
            folds_only,
            folded,
            opened: None,
            $($field: $value,)*
        }
    }};
}

/// The rows at `keys` the list `request` shows, in its order, and, when the
/// caller names a command in `acting`, only the rows that offer it.
pub(super) fn selected(
    sql: &impl bae_store::WriterRead,
    request: &ImportListRequest,
    keys: &[CandidateKey],
    acting: Option<CandidateAction>,
) -> Result<SelectedRows, DbError> {
    // Combining acts on every selected row, whatever each offers.
    let acting = match acting {
        Some(CandidateAction::Combine) | None => None,
        Some(action) => Some(action_flag(action)),
    };
    let params = shown_params!(
        SelectedParams,
        request,
        keys: crate::tables::key_hex_list(keys.iter()),
        acting: acting
    );
    let mut selected = SelectedRows::default();
    for row in statement_rows(sql, &params)? {
        match Item::of(&row.item)? {
            Item::Candidate => {
                let name = row.name.ok_or_else(|| {
                    DbError::unreadable("a selected release's name", None::<String>)
                })?;
                selected.candidates.push(SelectedCandidate { candidate_key: row.key.into(), name });
            }
            Item::Invalid | Item::Group => selected.invalid.push(row.key.into()),
        }
    }
    Ok(selected)
}

/// A shown release's key, which every row of a list's releases has.
fn shown_key(key: Option<Vec<u8>>) -> Result<CandidateKey, DbError> {
    key.map(CandidateKey::from)
        .ok_or_else(|| DbError::unreadable("a shown release's key", None::<String>))
}

/// Every release the list `request` shows, loaded by a surface or not.
fn shown_keys(
    sql: &impl bae_store::WriterRead,
    request: &ImportListRequest,
) -> Result<Vec<CandidateKey>, DbError> {
    let params = shown_params!(ShownKeysParams, request);
    statement_rows(sql, &params)?.into_iter().map(|row| shown_key(row.key)).collect()
}

/// The releases the list `request` shows from the release `from` to the
/// release `to`.
fn keys_between(
    sql: &impl bae_store::WriterRead,
    request: &ImportListRequest,
    from: &CandidateKey,
    to: &CandidateKey,
) -> Result<Vec<CandidateKey>, DbError> {
    let params = shown_params!(KeysBetweenParams, request, first: from.clone(), last: to.clone());
    statement_rows(sql, &params)?.into_iter().map(|row| shown_key(row.key)).collect()
}

/// Where the release at `key` sits in the list `request` shows; `None` when
/// the list does not show it.
pub(super) fn position_of(
    sql: &impl bae_store::WriterRead,
    request: &ImportListRequest,
    key: &CandidateKey,
) -> Result<Option<u64>, DbError> {
    let params = shown_params!(ShownPositionParams, request, key: key.clone());
    statement_optional(sql, &params)?
        .map(|row| to_u64(row.position, "an import list position"))
        .transpose()
}

/// What showing the list `after` in place of the list `before` comes to for
/// the selected `keys`. Another tab, filter or search narrows the list anew:
/// it opens at the first of `keys` it shows, or at its top, and the keys it
/// doesn't show leave the selection. Another order or another fold shows the
/// same view: the list keeps the person's place, and the keys a fold hid
/// leave the selection.
pub(super) fn view_change(
    sql: &impl bae_store::WriterRead,
    before: &ImportListRequest,
    after: &ImportListRequest,
    keys: &[CandidateKey],
) -> Result<ImportListViewChange, DbError> {
    let narrowed_anew = before.view.tab != after.view.tab
        || before.view.search != after.view.search
        || before.entry() != after.entry();
    match narrowed_anew {
        true => opening(sql, after, keys),
        false => Ok(ImportListViewChange {
            opening: ImportListOpening::InPlace,
            left: hidden_by(sql, before, after, keys)?,
        }),
    }
}

/// Where the list `request` shows opens: the position of the first of `keys`
/// it shows, or its top when it shows none; and the keys it doesn't show,
/// which leave the selection.
fn opening(
    sql: &impl bae_store::WriterRead,
    request: &ImportListRequest,
    keys: &[CandidateKey],
) -> Result<ImportListViewChange, DbError> {
    let params =
        shown_params!(OpeningParams, request, keys: crate::tables::key_hex_list(keys.iter()));
    let row = statement_row(sql, &params)?;
    let opening = match row.first_position {
        Some(position) => {
            ImportListOpening::At { position: to_u64(position, "an import list position")? }
        }
        None => ImportListOpening::Top,
    };
    let left = row.left_keys.into_iter().map(|left| CandidateKey::from(left.key)).collect();
    Ok(ImportListViewChange { opening, left })
}

/// The keys of `keys` whose rows the list `before` showed and the list
/// `after` doesn't: what a fold hid. The two requests show the same view.
fn hidden_by(
    sql: &impl bae_store::WriterRead,
    before: &ImportListRequest,
    after: &ImportListRequest,
    keys: &[CandidateKey],
) -> Result<Vec<CandidateKey>, DbError> {
    let ListBinding { folds_only: folds_only_before, folded: folded_before, .. } =
        ListBinding::of(before);
    let ListBinding { tab, filter, filter_text, folds_only, folded } = ListBinding::of(after);
    let params = HiddenByFoldsParams {
        tab,
        filter,
        filter_text,
        folds_only_before,
        folded_before,
        folds_only,
        folded,
        keys: crate::tables::key_hex_list(keys.iter()),
    };
    Ok(statement_rows(sql, &params)?.into_iter().map(|row| row.key.into()).collect())
}

impl ImportTables {
    /// The rows at `keys` the list `request` shows, in its order: what a bulk
    /// action runs over. When the caller names a command in `acting`, the
    /// method returns only the rows that offer it.
    pub async fn load_import_list_selected(
        &self,
        request: ImportListRequest,
        keys: Vec<CandidateKey>,
        acting: Option<CandidateAction>,
    ) -> Result<SelectedRows, DbError> {
        self.store.read(move |sql| selected(&sql, &request, &keys, acting)).await
    }

    /// The keys of every release the list `request` shows, loaded by a
    /// surface or not: what Select All selects.
    pub async fn import_list_keys(
        &self,
        request: ImportListRequest,
    ) -> Result<Vec<CandidateKey>, DbError> {
        self.store.read(move |sql| shown_keys(&sql, &request)).await
    }

    /// The releases the list `request` shows from the release `from` to the
    /// release `to`, both included, in the list's order: what Shift-clicking
    /// a row selects. When the list shows only one of `from` and `to`, the
    /// method returns that release alone. When it shows neither, the method
    /// returns no keys.
    pub async fn import_list_keys_between(
        &self,
        request: ImportListRequest,
        from: CandidateKey,
        to: CandidateKey,
    ) -> Result<Vec<CandidateKey>, DbError> {
        self.store.read(move |sql| keys_between(&sql, &request, &from, &to)).await
    }

    /// What showing the list `after` in place of the list `before` comes
    /// to for the keys of `selected`: where it opens, and which of them leave
    /// the selection.
    pub async fn import_list_view_change(
        &self,
        before: ImportListRequest,
        after: ImportListRequest,
        selected: Vec<CandidateKey>,
    ) -> Result<ImportListViewChange, DbError> {
        self.store.read(move |sql| view_change(&sql, &before, &after, &selected)).await
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
