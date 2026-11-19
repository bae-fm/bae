//! A group header's count of its members, as a window reads it: from the
//! group counts kept beside the placement, against a COUNT over the
//! placement, and by a plan that walks none of the group's members.

use super::*;
use crate::tables::import_list::tests::*;
use crate::{PendingFilter, TriageTab};
use std::collections::BTreeMap;

/// Three groups in two of Found's entries: `A` with members in both, `B`
/// with members in one, and `C`, a release with one found below it.
async fn grouped() -> Library {
    let lib = Library::new().await;
    let failed: Write = ["A/2", "B/1", "B/2"]
        .into_iter()
        .map(|display_path| lib.verdict(display_path, "failed", Some(11)))
        .sum();
    lib.run(
        scanned_with_states(&lib, &["A/1", "A/2", "A/3", "B/1", "B/2", "C", "C/Bonus", "D"])
            + failed,
    )
    .await;
    lib
}

/// Each header a read holds: its group's folder and its count.
fn header_counts(read: &crate::list::ImportListRead) -> BTreeMap<String, i64> {
    read.windows
        .iter()
        .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
        .filter_map(|item| match item {
            ImportListItem::GroupHeader(header) => {
                Some((header.group.key.relative_folder.text(), i64::from(header.entry_count)))
            }
            _ => None,
        })
        .collect()
}

/// Under every filter entry, with every group open and with every group
/// folded shut, each header counts its group's members that the entry keeps,
/// as a COUNT over the placement says, and every group holding such a member
/// has a header.
#[tokio::test]
async fn each_header_counts_its_members_in_the_placement() {
    let lib = grouped().await;
    let placed = lib.placed_group_counts().await;
    for filter in [
        PendingFilter::All,
        PendingFilter::NeedsYou,
        PendingFilter::NotLookedUp,
        PendingFilter::Identified,
    ] {
        let mut kept = BTreeMap::new();
        for (_root, group, entry, count) in &placed {
            if filter == PendingFilter::All || entry == pending_filter_column(filter) {
                *kept.entry(group.clone()).or_insert(0) += count;
            }
        }
        if filter == PendingFilter::All {
            assert_eq!(
                kept,
                BTreeMap::from([("A".to_string(), 3), ("B".to_string(), 2), ("C".to_string(), 2)]),
                "the placement holds every group's members"
            );
        }
        let list = ImportListRequest::of(under(TriageTab::Pending, filter));
        for list in [list.clone(), list.folding_all()] {
            let what = format!("{filter:?}, folds {:?}", list.folds);
            assert_eq!(header_counts(&lib.read(&list).await), kept, "{what}");
        }
    }
}

/// With no search, a window reads each header's count from the group
/// counts, by the group's key and, under an entry, the entry too: the
/// subquery that counts reads the group counts alone and no placement row.
#[tokio::test]
async fn a_window_counts_its_headers_from_the_group_counts() {
    let lib = grouped().await;
    let views = [view(TriageTab::Pending), under(TriageTab::Pending, PendingFilter::NeedsYou)];
    let plans = lib
        .db
        .read(move |sql| {
            views
                .into_iter()
                .map(|view| {
                    let request = ImportListRequest::of(view.clone());
                    let plan =
                        window_query(&request, &ListWindow::top(50), |items, statement, named| {
                            items.query(
                                &sql,
                                &format!("EXPLAIN QUERY PLAN {statement}"),
                                named,
                                |row| {
                                    Ok((
                                        row.get::<_, i64>(0)?,
                                        row.get::<_, i64>(1)?,
                                        row.get::<_, String>(3)?,
                                    ))
                                },
                            )
                        })?
                        .expect("the list has rows");
                    Ok((view, plan))
                })
                .collect::<Result<Vec<_>, DbError>>()
        })
        .await
        .unwrap();
    for (view, plan) in plans {
        let counting: Vec<&(i64, i64, String)> =
            plan.iter().filter(|(_, _, step)| step.starts_with("SEARCH gc ")).collect();
        let [(_, subquery, step)] = counting.as_slice() else {
            panic!("{view:?}: one step reads the group counts: {plan:#?}");
        };
        let by_key = match view.pending_filter {
            PendingFilter::All => "(watched_folder_path=? AND group_folder=?)",
            _ => "(watched_folder_path=? AND group_folder=? AND pending_entry=?)",
        };
        assert!(step.contains(by_key), "{view:?}: the group counts are read by key: {step}");
        let in_subquery: Vec<&String> = plan
            .iter()
            .filter(|(_, parent, _)| parent == subquery)
            .map(|(_, _, step)| step)
            .collect();
        assert_eq!(
            in_subquery,
            [step],
            "{view:?}: the header's count reads the group counts alone: {plan:#?}"
        );
    }
}
