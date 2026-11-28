//! A window from a list's top holds the list's first rows, in every order of
//! every library list: the same rows, with the same keys, as the window from
//! the key of the row at position 0.

use crate::*;
use bae_store::list_windows::{Keyed, OrderKey};

/// The keys of a window's rows, in its order.
fn keys<R>(rows: &[Keyed<R>]) -> Vec<OrderKey> {
    rows.iter().map(|(_, key)| key.clone()).collect()
}

/// Both ways of each of `fields`, each alone and then before the next field.
fn sorts<F: Copy>(fields: &[F]) -> Vec<Vec<(F, SortDirection)>> {
    let mut sorts = Vec::new();
    for (place, &field) in fields.iter().enumerate() {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            sorts.push(vec![(field, direction)]);
            let next = fields[(place + 1) % fields.len()];
            sorts.push(vec![(field, direction), (next, SortDirection::Descending)]);
        }
    }
    sorts
}

/// Asserts that the window of `limit` rows from the top, read by `window`,
/// starts with the row at position 0, whose key `key_at` reads, and holds
/// what the window from that key holds.
fn assert_top_is_first_rows<R>(
    what: &str,
    key_at: impl Fn() -> Result<Option<OrderKey>, DbError>,
    window: impl Fn(&ListWindow) -> Result<WindowRead<R>, DbError>,
) -> Result<(), DbError> {
    let limit = 20;
    let first = key_at()?.unwrap_or_else(|| panic!("{what}: the list has rows"));
    let top = window(&ListWindow::top(limit))?;
    let from_first = window(&ListWindow { from: Some(first), limit })?;
    assert!(!top.rows.is_empty(), "{what}: the window from the top holds no rows");
    assert_eq!(keys(&top.rows), keys(&from_first.rows), "{what}");
    Ok(())
}

#[tokio::test]
async fn every_lists_window_from_the_top_holds_its_first_rows() {
    let (db, _tmp) = crate::tests::library().await;
    db.read(|sql| {
        for sort in sorts(&[ArtistSortField::Name, ArtistSortField::AlbumCount]) {
            let criteria: Vec<_> = sort
                .iter()
                .map(|&(field, direction)| ArtistSortCriterion { field, direction })
                .collect();
            let sort = crate::artist::artist_sort(&criteria);
            assert_top_is_first_rows(
                &format!("artists {criteria:?}"),
                || crate::artist::artist_key_at_on(&sql, &sort, 0),
                |window| crate::artist::artist_window_on(&sql, &sort, window),
            )?;
        }
        for sort in sorts(&[
            ComposerSortField::Name,
            ComposerSortField::WorkCount,
            ComposerSortField::LinkedReleaseCount,
        ]) {
            let criteria: Vec<_> = sort
                .iter()
                .map(|&(field, direction)| ComposerSortCriterion { field, direction })
                .collect();
            let sort = crate::composer::composer_sort(&criteria);
            assert_top_is_first_rows(
                &format!("composers {criteria:?}"),
                || crate::composer::composer_key_at_on(&sql, &sort, 0),
                |window| crate::composer::composer_window_on(&sql, &sort, window),
            )?;
        }
        let album_fields = [
            AlbumSortField::Title,
            AlbumSortField::Artist,
            AlbumSortField::Year,
            AlbumSortField::DateAdded,
        ];
        let mut album_sorts = sorts(&album_fields);
        album_sorts.push(Vec::new());
        for sort in album_sorts {
            let criteria: Vec<_> = sort
                .iter()
                .map(|&(field, direction)| AlbumSortCriterion { field, direction })
                .collect();
            for group_by_artist in [false, true] {
                let order = crate::album::AlbumOrder::new(&criteria, group_by_artist);
                assert_top_is_first_rows(
                    &format!("albums {criteria:?}, grouped {group_by_artist}"),
                    || crate::album::album_key_at_on(&sql, &order, 0),
                    |window| crate::album::album_window_on(&sql, &order, window),
                )?;
            }
        }
        let storage_fields = [
            StorageSortField::AlbumTitle,
            StorageSortField::ArtistNames,
            StorageSortField::Media,
            StorageSortField::FileCount,
            StorageSortField::TotalSize,
            StorageSortField::Storage,
            StorageSortField::DateAdded,
        ];
        for sort in sorts(&storage_fields) {
            let (field, direction) = sort[0];
            let criterion = StorageSortCriterion { field, direction };
            for (filter, search) in [
                (StorageFilter::All, ""),
                (StorageFilter::Local, ""),
                (StorageFilter::All, "album"),
            ] {
                let list = crate::storage::StorageList::new(&criterion, filter, search);
                assert_top_is_first_rows(
                    &format!("storage {criterion:?} {filter:?} {search:?}"),
                    || crate::storage::storage_key_at_on(&sql, &list, 0),
                    |window| crate::storage::storage_window_on(&sql, &list, window),
                )?;
            }
        }
        Ok(())
    })
    .await
    .unwrap();
}
