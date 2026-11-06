//! Album links as rows: the albums on another catalog that statements say a
//! MusicBrainz release group is.
//!
//! A catalog release's rows and a verdict's match rows carry the albums read
//! about the release's album, so a list read back from the store groups as it
//! did. Which statement named an album is not kept: nothing past the reading
//! decides by it. `release_group_album_link` keeps the albums any reading
//! found a group to be, beyond the list that read it, so a release of either
//! album carries the other among its records — a Discogs release whose own
//! documents never reach the release group still names it.
//!
//! Its table is `album_link_rows.sql`.

pub(super) const SQL: &str = include_str!("album_link_rows.sql");

use super::*;
use bae_catalogs::album_links::GroupAlbum;
use bae_catalogs::records::MetadataRef;

fn catalog_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored).map_err(DbError::Message)
}

/// One stored album link row's columns, as read.
#[derive(serde::Deserialize)]
pub(crate) struct AlbumLinkRow {
    pub(super) catalog: String,
    pub(super) key: String,
}

impl AlbumLinkRow {
    /// The album the row names.
    pub(crate) fn album(self) -> Result<MetadataRef, DbError> {
        Ok(MetadataRef::new(catalog_of(&self.catalog)?, self.key))
    }
}

const GROUP_LINK_COLUMNS: &str = "release_group, catalog, key";

fn read_group_link(row: &Row<'_>) -> rusqlite::Result<(String, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

/// Every album kept for any of `albums`: one a MusicBrainz release group
/// among them was found to be, or one of the other catalogs' albums among
/// them some group was found to be.
pub(super) fn group_albums_on<S: QueryRows>(
    sql: &S,
    albums: &[MetadataRef],
) -> Result<Vec<GroupAlbum>, DbError> {
    let mut kept: Vec<GroupAlbum> = Vec::new();
    for album in albums {
        let rows = match album.catalog {
            Catalog::MusicBrainz => sql.query(
                &format!(
                    "SELECT {GROUP_LINK_COLUMNS} FROM release_group_album_link \
                     WHERE release_group = ? ORDER BY catalog, key"
                ),
                params![album.key],
                read_group_link,
            )?,
            other => sql.query(
                &format!(
                    "SELECT {GROUP_LINK_COLUMNS} FROM release_group_album_link \
                     WHERE catalog = ? AND key = ? ORDER BY release_group"
                ),
                params![other.as_str(), album.key],
                read_group_link,
            )?,
        };
        for (group, catalog, key) in rows {
            let group_album =
                GroupAlbum { group, album: MetadataRef::new(catalog_of(&catalog)?, key) };
            if !kept.contains(&group_album) {
                kept.push(group_album);
            }
        }
    }
    Ok(kept)
}

/// The albums the kept group albums join to a stored release's own albums —
/// its group, and the albums its records name — on a catalog none of its
/// records reaches. The release's own catalog is never one: its album is its
/// group.
pub(super) fn joined_albums_on<S: QueryRows>(
    sql: &S,
    catalog: Catalog,
    source_group_id: Option<&str>,
    records: &[bae_catalogs::records::ReleaseRecord],
) -> Result<Vec<MetadataRef>, DbError> {
    let albums: Vec<MetadataRef> = source_group_id
        .map(|group| MetadataRef::new(catalog, group))
        .into_iter()
        .chain(records.iter().filter_map(bae_catalogs::records::ReleaseRecord::album_ref))
        .collect();
    let mut joined: Vec<MetadataRef> = Vec::new();
    for group_album in group_albums_on(sql, &albums)? {
        for album in &albums {
            let Some(other) = group_album.other_than(album) else {
                continue;
            };
            let reached = other.catalog == catalog
                || records.iter().any(|record| record.catalog() == other.catalog)
                || joined.iter().any(|named| named.catalog == other.catalog);
            if !reached {
                joined.push(other);
            }
        }
    }
    Ok(joined)
}

impl ImportTables {
    /// Keep the albums reading these release groups found each to be,
    /// replacing whatever an earlier reading of the same group found. A group
    /// whose reading named nothing keeps nothing.
    pub async fn replace_group_album_links(
        &self,
        read: Vec<(String, Vec<MetadataRef>)>,
    ) -> Result<(), DbError> {
        self.store.call(move |sql| replace_group_album_links_on(sql, &read)).await
    }
}

/// Keep, inside the caller's write, the albums reading these release groups
/// found each to be, replacing whatever an earlier reading of the same group
/// found.
pub(super) fn replace_group_album_links_on(
    sql: &CovenWrite<'_>,
    read: &[(String, Vec<MetadataRef>)],
) -> Result<(), DbError> {
    for (group, albums) in read {
        sql.execute(
            "DELETE FROM release_group_album_link WHERE release_group = ?",
            params![group],
        )?;
        for album in albums {
            sql.execute(
                &format!(
                    "INSERT INTO release_group_album_link ({GROUP_LINK_COLUMNS}) VALUES (?, ?, ?)"
                ),
                params![group, album.catalog.as_str(), album.key],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: every keep of an album's links, in the
    /// library under `directory`, fails, as a full disk fails it.
    pub(crate) async fn fail_album_link_writes_for_test(directory: &std::path::Path) {
        let directory = directory.to_path_buf();
        tokio::task::spawn_blocking(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "release_group_album_link",
                "CREATE TRIGGER fail_album_link_write BEFORE INSERT ON release_group_album_link \
                 BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;",
            )
        })
        .await
        .unwrap();
    }
}
