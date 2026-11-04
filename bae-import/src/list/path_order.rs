//! The order a watched folder's releases run in by path, for the scan writes
//! that hand back the keys of the releases they store.

use crate::folder_scanner::ScanItem;
use crate::CandidateKey;

/// The keys of the releases among `items` in path order: by watched folder
/// path, then by each release's folder below it, as the list shows them — the
/// order the list runs one watched folder's releases in when it orders by path.
pub(crate) fn release_keys_in_path_order<'a>(
    items: impl IntoIterator<Item = &'a ScanItem>,
) -> Vec<CandidateKey> {
    let mut releases: Vec<(String, String, CandidateKey)> = items
        .into_iter()
        .map(|item| {
            let watched_folder_path = match item {
                ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                    &candidate.watched_folder_path
                }
                ScanItem::Invalid(candidate) => &candidate.watched_folder_path,
            };
            (coven::path_text(watched_folder_path), item.relative_folder().text(), item.key())
        })
        .collect();
    releases.sort_by(|left, right| {
        bae_db::collation::natural_order(&left.0, &right.0)
            .then_with(|| bae_db::collation::natural_order(&left.1, &right.1))
    });
    releases.into_iter().map(|(_, _, key)| key).collect()
}
