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
    Catalog::from_str(stored)
        .map_err(|_unknown| DbError::unreadable("a stored album link's catalog", stored))
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

/// For each of `wanted`, every album kept for any of its albums: one a
/// MusicBrainz release group among them was found to be, or one of the
/// other catalogs' albums among them some group was found to be. Each is
/// listed once, album by album in their order: a group's albums by catalog
/// and key, an album's groups by id. One statement whatever `wanted` holds,
/// none when it names no album.
pub(super) fn group_albums_of_on<S: coven::SqlRead>(
    sql: &S,
    wanted: &[Vec<MetadataRef>],
) -> Result<Vec<Vec<GroupAlbum>>, DbError> {
    // Each album is asked once, at its place among the albums asked.
    let mut albums: Vec<&MetadataRef> = Vec::new();
    let mut asked_at: std::collections::HashMap<&MetadataRef, usize> =
        std::collections::HashMap::new();
    for album in wanted.iter().flatten() {
        asked_at.entry(album).or_insert_with(|| {
            albums.push(album);
            albums.len() - 1
        });
    }
    if albums.is_empty() {
        return Ok(vec![Vec::new(); wanted.len()]);
    }
    let asked = serde_json::Value::Array(
        albums.iter().map(|album| serde_json::json!([album.catalog.as_str(), album.key])).collect(),
    );
    const LINK_COLUMNS: &str = "link.release_group, link.catalog, link.key";
    let rows = sql.query(
        &format!(
            "WITH asked (at, catalog, key) AS ( \
                 SELECT key, value ->> 0, value ->> 1 FROM json_each(?1)) \
             SELECT asked.at, {LINK_COLUMNS} FROM asked \
             JOIN release_group_album_link AS link \
                 ON asked.catalog = ?2 AND link.release_group = asked.key \
             UNION ALL \
             SELECT asked.at, {LINK_COLUMNS} FROM asked \
             JOIN release_group_album_link AS link \
                 ON asked.catalog <> ?2 AND link.catalog = asked.catalog AND link.key = asked.key \
             ORDER BY 1, 3, 4, 2"
        ),
        params![asked.to_string(), Catalog::MusicBrainz.as_str()],
        |row| Ok((row.get::<_, usize>(0)?, read_group_link(row, 1)?)),
    )?;
    let mut kept_for: Vec<Vec<GroupAlbum>> = vec![Vec::new(); albums.len()];
    for (at, (group, catalog, key)) in rows {
        kept_for[at]
            .push(GroupAlbum { group, album: MetadataRef::new(catalog_of(&catalog)?, key) });
    }
    Ok(wanted
        .iter()
        .map(|albums_of| {
            let mut kept: Vec<GroupAlbum> = Vec::new();
            for album in albums_of {
                for group_album in &kept_for[asked_at[album]] {
                    if !kept.contains(group_album) {
                        kept.push(group_album.clone());
                    }
                }
            }
            kept
        })
        .collect())
}

fn read_group_link(row: &Row<'_>, first: usize) -> rusqlite::Result<(String, String, String)> {
    Ok((row.get(first)?, row.get(first + 1)?, row.get(first + 2)?))
}

/// A stored release as far as which albums join it: its catalog, the group
/// its catalog files it under, and its records.
pub(super) struct ReleaseAlbums<'a> {
    pub(super) catalog: Catalog,
    pub(super) source_group_id: Option<&'a str>,
    pub(super) records: &'a [bae_catalogs::records::ReleaseRecord],
}

impl ReleaseAlbums<'_> {
    /// The release's own albums: its group, and the albums its records name.
    fn albums(&self) -> Vec<MetadataRef> {
        self.source_group_id
            .map(|group| MetadataRef::new(self.catalog, group))
            .into_iter()
            .chain(self.records.iter().filter_map(bae_catalogs::records::ReleaseRecord::album_ref))
            .collect()
    }

    /// The albums `kept`, the group albums kept for its own albums, join to
    /// them on a catalog none of its records reaches.
    fn joined(&self, albums: &[MetadataRef], kept: &[GroupAlbum]) -> Vec<MetadataRef> {
        let mut joined: Vec<MetadataRef> = Vec::new();
        for group_album in kept {
            for album in albums {
                let Some(other) = group_album.other_than(album) else {
                    continue;
                };
                let reached = other.catalog == self.catalog
                    || self.records.iter().any(|record| record.catalog() == other.catalog)
                    || joined.iter().any(|named| named.catalog == other.catalog);
                if !reached {
                    joined.push(other);
                }
            }
        }
        joined
    }
}

/// For each of `releases`, the albums the kept group albums join to the
/// stored release's own albums — its group, and the albums its records name
/// — on a catalog none of its records reaches. The release's own catalog is
/// never one: its album is its group. One statement whatever `releases`
/// holds.
pub(super) fn joined_albums_of_on<S: coven::SqlRead>(
    sql: &S,
    releases: &[ReleaseAlbums<'_>],
) -> Result<Vec<Vec<MetadataRef>>, DbError> {
    let albums: Vec<Vec<MetadataRef>> = releases.iter().map(ReleaseAlbums::albums).collect();
    let kept = group_albums_of_on(sql, &albums)?;
    Ok(releases
        .iter()
        .zip(&albums)
        .zip(&kept)
        .map(|((release, albums), kept)| release.joined(albums, kept))
        .collect())
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
        bae_util::test_blocking::on_blocking_pool(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "release_group_album_link",
                "CREATE TRIGGER fail_album_link_write BEFORE INSERT ON release_group_album_link \
                 BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;",
            )
        })
        .await;
    }
}
