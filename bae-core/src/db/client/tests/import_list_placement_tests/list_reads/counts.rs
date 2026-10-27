//! What a list counts against what it lists: the count, the narrowed count and
//! each header's count are read from the counts kept beside the placement
//! wherever no text filter reads the rows, and must say what a read of the
//! whole list holds.

use super::*;

/// Two groups and two releases outside any, in two of Found's entries.
async fn grouped_in_two_entries() -> Library {
    let lib = Library::new().await;
    let releases = ["A/1", "A/2", "A/3", "B/1", "B/2", "C", "D"];
    let mut sql = scanned_with_states(&lib, &releases);
    for display_path in ["A/2", "B/1", "B/2", "C"] {
        sql.push_str(&lib.verdict(display_path, "failed", Some(11)));
    }
    lib.run(&sql).await;
    lib
}

/// The releases a read holds, by display path.
fn releases(projection: &ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.display_path.clone()),
            _ => None,
        })
        .collect()
}

/// Under every filter, with no group, one or both folded shut, a list counts
/// the items a read of all of it holds; its narrowed count is the releases its
/// filters keep, folded ones included; and each header counts the releases
/// its filters keep in its group.
#[tokio::test]
async fn a_list_counts_what_it_lists() {
    let lib = grouped_in_two_entries().await;
    for filter in [
        PendingFilter::All,
        PendingFilter::NeedsYou,
        PendingFilter::NotLookedUp,
        PendingFilter::Identified,
    ] {
        for text in ["", "1"] {
            let view = ImportListView {
                filter_text: text.to_string(),
                ..under(TriageTab::Pending, filter)
            };
            let kept = releases(&lib.shown(view.clone()).await);
            for folded in [&[][..], &["A"][..], &["A", "B"][..]] {
                let list = lib.open(view.clone()).await;
                for group in folded {
                    lib.db
                        .set_import_list_group_folded(list, lib.group_key(group), true)
                        .await
                        .unwrap();
                }
                let read = lib.read(list).await;
                let what = format!("{filter:?}, text {text:?}, folded {folded:?}");
                assert_eq!(read.total_count as usize, sequence(&read).len(), "{what}");
                match filter != PendingFilter::All || !text.is_empty() {
                    true => assert_eq!(
                        read.summary
                            .narrowed
                            .map(|narrowed| narrowed.shown as usize),
                        Some(kept.len()),
                        "{what}"
                    ),
                    false => assert_eq!(read.summary.narrowed, None, "{what}"),
                }
                for item in read.windows.iter().flat_map(|window| &window.items) {
                    let ImportListItem::GroupHeader {
                        group, entry_count, ..
                    } = item
                    else {
                        continue;
                    };
                    let members = kept
                        .iter()
                        .filter(|path| path.starts_with(&format!("{}/", group.name)))
                        .count();
                    assert_eq!(
                        *entry_count as usize, members,
                        "{what}, group {}",
                        group.name
                    );
                }
            }
        }
    }
}
