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

use super::*;
use bae_catalogs::album_links::{AlbumLinks, GroupAlbum};
use bae_catalogs::records::MetadataRef;
use bae_catalogs::source_release::SourceRelease;
use bae_store::{statement_row, WriteStatements};

fn catalog_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored)
        .map_err(|_unknown| DbError::unreadable("a stored album link's catalog", stored))
}

/// One stored album link row's columns, as read.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
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

/// The stored `album_links` values, one per [`AlbumLinks`] shape.
const ALBUM_LINKS_NOT_ASKED: &str = "not_asked";
const ALBUM_LINKS_READ: &str = "read";
const ALBUM_LINKS_UNREAD: &str = "unread";

/// The stored `album_links` value of `links`.
pub(super) fn album_links_column(links: &AlbumLinks) -> &'static str {
    match links {
        AlbumLinks::NotAsked => ALBUM_LINKS_NOT_ASKED,
        AlbumLinks::Read(_) => ALBUM_LINKS_READ,
        AlbumLinks::Unread => ALBUM_LINKS_UNREAD,
    }
}

/// The album links of `owner` that its stored `album_links` value and its
/// album link rows hold. Only read links have rows.
pub(super) fn album_links_of(
    owner: &str,
    stored: &str,
    rows: Vec<AlbumLinkRow>,
) -> Result<AlbumLinks, DbError> {
    Ok(match stored {
        ALBUM_LINKS_READ => {
            AlbumLinks::Read(rows.into_iter().map(AlbumLinkRow::album).collect::<Result<_, _>>()?)
        }
        ALBUM_LINKS_NOT_ASKED | ALBUM_LINKS_UNREAD if !rows.is_empty() => {
            return Err(DbError::unreadable(
                format!("the album link rows of {owner}, whose album links are {stored}"),
                rows.len(),
            ))
        }
        ALBUM_LINKS_NOT_ASKED => AlbumLinks::NotAsked,
        ALBUM_LINKS_UNREAD => AlbumLinks::Unread,
        other => return Err(DbError::unreadable(format!("the album links of {owner}"), other)),
    })
}

coven::write!(
    DeleteGroupAlbumLinks(release_group: &'p str),
    r#"DELETE FROM release_group_album_link WHERE release_group = :release_group"#
);

coven::write!(
    InsertGroupAlbumLink(release_group: &'p str, catalog: &'static str, key: &'p str),
    r#"
    INSERT INTO release_group_album_link (release_group, catalog, key)
    VALUES (:release_group, :catalog, :key)
"#
);

// For each of `wanted`, every album kept for any of the albums `asked` gives
// it: one a MusicBrainz release group among them was found to be, or one of
// the other catalogs' albums among them some group was found to be. They are
// listed album by album in the order of `ord`: a group's albums by catalog
// and key, an album's groups by id.
coven::fragment!(
    KeptGroupAlbums with wanted(at INTEGER),
        asked(at INTEGER, ord INTEGER, catalog TEXT, key TEXT)
    {
        struct StoredGroupAlbum { release_group: String, catalog: String, key: String }
        at: i64,
        albums: json<Vec<StoredGroupAlbum>>
    },
    r#"
    SELECT wanted.at,
           (SELECT json_group_array(json_object('release_group', matched.release_group,
                       'catalog', matched.catalog, 'key', matched.key)
                       ORDER BY matched.ord, matched.catalog, matched.key,
                           matched.release_group)
            FROM (SELECT asked.at, asked.ord, link.release_group, link.catalog, link.key
                  FROM asked
                  JOIN release_group_album_link AS link
                      ON asked.catalog = 'musicbrainz' AND link.release_group = asked.key
                  UNION ALL
                  SELECT asked.at, asked.ord, link.release_group, link.catalog, link.key
                  FROM asked
                  JOIN release_group_album_link AS link
                      ON asked.catalog <> 'musicbrainz' AND link.catalog = asked.catalog
                     AND link.key = asked.key) AS matched
            WHERE matched.at = wanted.at) AS albums
    FROM wanted
"#
);

coven::read!(
    writer KeptForAlbums(albums: String),
    r#"
    SELECT kept.*
    FROM @crate::tables::album_link_rows::KeptGroupAlbums(
        wanted: (SELECT 0 AS at),
        asked: (SELECT 0 AS at, listed.key AS ord, listed.value ->> 0 AS catalog,
                       listed.value ->> 1 AS key
                FROM json_each(:albums) AS listed)) AS kept
"#
);

/// The group albums `kept` holds, in its order.
pub(super) fn group_albums_of(kept: KeptGroupAlbums) -> Result<Vec<GroupAlbum>, DbError> {
    kept.albums
        .into_iter()
        .map(|stored| {
            Ok(GroupAlbum {
                group: stored.release_group,
                album: MetadataRef::new(catalog_of(&stored.catalog)?, stored.key),
            })
        })
        .collect()
}

/// Every album kept for any of `albums`, as [`KeptGroupAlbums`] lists them,
/// in one statement.
pub(super) fn kept_group_albums_on<S: bae_store::WriterRead>(
    sql: &S,
    albums: &[MetadataRef],
) -> Result<Vec<GroupAlbum>, DbError> {
    let albums = serde_json::Value::Array(
        albums.iter().map(|album| serde_json::json!([album.catalog.as_str(), album.key])).collect(),
    );
    let request = KeptForAlbumsParams { albums: albums.to_string() };
    group_albums_of(statement_row(sql, &request)?.kept)
}

/// `release`'s own albums: the one its catalog files it under, and those its
/// records name.
pub(super) fn own_albums(release: &SourceRelease) -> Vec<MetadataRef> {
    release
        .source_group_id
        .as_deref()
        .map(|group| MetadataRef::new(release.release.catalog.catalog(), group))
        .into_iter()
        .chain(
            release
                .other_records
                .iter()
                .filter_map(bae_catalogs::records::ReleaseRecord::album_ref),
        )
        .collect()
}

/// The albums `kept`, the group albums kept for `release`'s own albums, join
/// to them on a catalog none of its records reaches. The release's own
/// catalog is never one: its album is its group.
pub(super) fn joined_albums(release: &SourceRelease, kept: &[GroupAlbum]) -> Vec<MetadataRef> {
    let catalog = release.release.catalog.catalog();
    let albums = own_albums(release);
    let mut joined: Vec<MetadataRef> = Vec::new();
    for group_album in kept {
        for album in &albums {
            let Some(other) = group_album.other_than(album) else {
                continue;
            };
            let reached = other.catalog == catalog
                || release.other_records.iter().any(|record| record.catalog() == other.catalog)
                || joined.iter().any(|named| named.catalog == other.catalog);
            if !reached {
                joined.push(other);
            }
        }
    }
    joined
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
/// found, and store again what each link claims whose claimed release joins
/// the albums kept before or now.
pub(super) fn replace_group_album_links_on(
    sql: &CovenWrite<'_>,
    read: &[(String, Vec<MetadataRef>)],
) -> Result<(), DbError> {
    let mut changed: Vec<MetadataRef> = Vec::new();
    for (group, albums) in read {
        let kept_before = sql.query(
            "SELECT catalog, key FROM release_group_album_link WHERE release_group = ?",
            params![group],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        changed.push(MetadataRef::new(Catalog::MusicBrainz, group.clone()));
        for (catalog, key) in kept_before {
            changed.push(MetadataRef::new(catalog_of(&catalog)?, key));
        }
        changed.extend(albums.iter().cloned());
        sql.run(&DeleteGroupAlbumLinksParams { release_group: group })?;
        for album in albums {
            sql.run(&InsertGroupAlbumLinkParams {
                release_group: group,
                catalog: album.catalog.as_str(),
                key: &album.key,
            })?;
        }
    }
    super::import_state::store_claims_of_albums_on(sql, &changed)
}

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: every keep of an album's links, in the
    /// library under `directory`, fails, as a full disk fails it.
    pub(crate) async fn fail_album_link_writes_for_test(directory: &std::path::Path) {
        let directory = directory.to_path_buf();
        bae_util::test_spawn::on_blocking_pool(move || {
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
