//! The order a watched folder's releases run in by path, for the scan writes
//! that hand back the keys of the releases they store.

use crate::import::folder_scanner::ScanItem;

/// The keys of the releases among `items` in path order: by watched folder
/// path, then by each release's path below it — the order the list runs one
/// watched folder's releases in when it orders by path. A folder's reading and
/// its sidecar files are no rows, so they have no key here.
pub(crate) fn release_keys_in_path_order<'a>(
    items: impl IntoIterator<Item = &'a ScanItem>,
) -> Vec<String> {
    let mut releases: Vec<(&str, &str, String)> = items
        .into_iter()
        .filter_map(|item| match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => Some((
                candidate.watched_folder_path.as_str(),
                candidate.display_path.as_str(),
                candidate.key(),
            )),
            ScanItem::Invalid(candidate) => Some((
                candidate.watched_folder_path.as_str(),
                candidate.display_path.as_str(),
                candidate.key(),
            )),
            ScanItem::Decided { .. } | ScanItem::Sidecar(_) => None,
        })
        .collect();
    releases.sort_by(|left, right| {
        crate::migrations::natural_order(left.0, right.0)
            .then_with(|| crate::migrations::natural_order(left.1, right.1))
    });
    releases.into_iter().map(|(_, _, key)| key).collect()
}
