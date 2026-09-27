//! Album links as rows: the statements that say what a MusicBrainz release
//! group is on another catalog.
//!
//! Two tables hold them in the one shape. A verdict's match rows carry what a
//! run's reading found about the match's album, so a list read back from the
//! store groups as it did. `release_group_album_link` keeps what any reading
//! found about a group, beyond the list that read it, so a release of either
//! album carries the other among its records — a Discogs release whose own
//! documents never reach the release group still names it.

use super::*;
use crate::import::album_links::{AlbumLink, AlbumStatement, GroupStatement};
use crate::import::MetadataRef;

/// The stored `stated` values, one per [`AlbumStatement`] shape.
const STATED_PAGE: &str = "page";
const STATED_WIKIDATA: &str = "wikidata";
const STATED_RELEASE: &str = "release";
const STATED_BARCODE: &str = "barcode";
const STATED_CATALOG_NUMBER: &str = "catalog_number";

fn catalog_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored).map_err(DbError::Message)
}

/// One stored album link row's columns, as read.
pub(super) struct AlbumLinkRow {
    pub(super) catalog: String,
    pub(super) key: String,
    pub(super) stated: String,
    pub(super) wikidata_item: Option<String>,
    pub(super) musicbrainz_release: Option<String>,
    pub(super) release_catalog: Option<String>,
    pub(super) release_key: Option<String>,
}

impl AlbumLinkRow {
    /// The link the row states. The tables' checks hold each statement's
    /// columns to its kind, so a row that breaks them is unreadable.
    pub(super) fn link(self) -> Result<AlbumLink, DbError> {
        let stated = match (
            self.stated.as_str(),
            self.wikidata_item,
            self.musicbrainz_release,
            self.release_catalog,
            self.release_key,
        ) {
            (STATED_PAGE, None, None, None, None) => AlbumStatement::Page,
            (STATED_WIKIDATA, Some(item), None, None, None) => AlbumStatement::Wikidata { item },
            (
                stated @ (STATED_RELEASE | STATED_BARCODE | STATED_CATALOG_NUMBER),
                None,
                Some(musicbrainz_release),
                Some(catalog),
                Some(key),
            ) => {
                let release = MetadataRef::new(catalog_of(&catalog)?, key);
                match stated {
                    STATED_RELEASE => AlbumStatement::Release {
                        musicbrainz_release,
                        twin: release,
                    },
                    STATED_BARCODE => AlbumStatement::Barcode {
                        musicbrainz_release,
                        release,
                    },
                    _ => AlbumStatement::CatalogNumber {
                        musicbrainz_release,
                        release,
                    },
                }
            }
            (other, ..) => {
                return Err(DbError::Message(format!(
                    "an album link row states {other:?} with columns that do not fit it"
                )))
            }
        };
        Ok(AlbumLink {
            album: MetadataRef::new(catalog_of(&self.catalog)?, self.key),
            stated,
        })
    }
}

/// The columns one statement is written as.
pub(super) struct StatementColumns<'a> {
    pub(super) stated: &'static str,
    pub(super) wikidata_item: Option<&'a str>,
    pub(super) musicbrainz_release: Option<&'a str>,
    /// The other catalog's release the statement pairs `musicbrainz_release`
    /// with.
    pub(super) release: Option<&'a MetadataRef>,
}

impl<'a> StatementColumns<'a> {
    pub(super) fn of(stated: &'a AlbumStatement) -> Self {
        match stated {
            AlbumStatement::Page => Self {
                stated: STATED_PAGE,
                wikidata_item: None,
                musicbrainz_release: None,
                release: None,
            },
            AlbumStatement::Wikidata { item } => Self {
                stated: STATED_WIKIDATA,
                wikidata_item: Some(item),
                musicbrainz_release: None,
                release: None,
            },
            AlbumStatement::Release {
                musicbrainz_release,
                twin,
            } => Self::of_pair(STATED_RELEASE, musicbrainz_release, twin),
            AlbumStatement::Barcode {
                musicbrainz_release,
                release,
            } => Self::of_pair(STATED_BARCODE, musicbrainz_release, release),
            AlbumStatement::CatalogNumber {
                musicbrainz_release,
                release,
            } => Self::of_pair(STATED_CATALOG_NUMBER, musicbrainz_release, release),
        }
    }

    fn of_pair(
        stated: &'static str,
        musicbrainz_release: &'a str,
        release: &'a MetadataRef,
    ) -> Self {
        Self {
            stated,
            wikidata_item: None,
            musicbrainz_release: Some(musicbrainz_release),
            release: Some(release),
        }
    }
}

const GROUP_LINK_COLUMNS: &str = "release_group, catalog, key, stated, wikidata_item, \
     musicbrainz_release, release_catalog, release_key";

fn read_group_link(row: &Row<'_>) -> coven::rusqlite::Result<(String, AlbumLinkRow)> {
    Ok((
        row.get(0)?,
        AlbumLinkRow {
            catalog: row.get(1)?,
            key: row.get(2)?,
            stated: row.get(3)?,
            wikidata_item: row.get(4)?,
            musicbrainz_release: row.get(5)?,
            release_catalog: row.get(6)?,
            release_key: row.get(7)?,
        },
    ))
}

/// Every statement kept about any of `albums`: one naming a MusicBrainz
/// release group among them as some album, or naming some group as one of
/// the other catalogs' albums among them.
pub(super) fn group_statements_on<S: QueryRows>(
    sql: &S,
    albums: &[MetadataRef],
) -> Result<Vec<GroupStatement>, DbError> {
    let mut statements: Vec<GroupStatement> = Vec::new();
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
        for (group, row) in rows {
            let statement = GroupStatement {
                group,
                link: row.link()?,
            };
            if !statements.contains(&statement) {
                statements.push(statement);
            }
        }
    }
    Ok(statements)
}

/// The albums kept statements join to a stored release's own albums — its
/// group, and the albums its records name — on a catalog none of its records
/// reaches. The release's own catalog is never one: its album is its group.
pub(super) fn joined_albums_on<S: QueryRows>(
    sql: &S,
    catalog: Catalog,
    source_group_id: Option<&str>,
    records: &[crate::import::ReleaseRecord],
) -> Result<Vec<MetadataRef>, DbError> {
    let albums: Vec<MetadataRef> = source_group_id
        .map(|group| MetadataRef::new(catalog, group))
        .into_iter()
        .chain(records.iter().filter_map(crate::import::ReleaseRecord::album_ref))
        .collect();
    let mut joined: Vec<MetadataRef> = Vec::new();
    for statement in group_statements_on(sql, &albums)? {
        for album in &albums {
            let Some(other) = statement.other_than(album) else {
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

impl Database {
    /// Keep what reading these release groups found each to be, replacing
    /// whatever an earlier reading of the same group found. A group whose
    /// reading named nothing keeps nothing.
    pub async fn replace_group_album_links(
        &self,
        read: Vec<(String, Vec<AlbumLink>)>,
    ) -> Result<(), DbError> {
        self.call(move |sql| {
            for (group, links) in &read {
                sql.execute(
                    "DELETE FROM release_group_album_link WHERE release_group = ?",
                    params![group],
                )?;
                for link in links {
                    let columns = StatementColumns::of(&link.stated);
                    sql.execute(
                        &format!(
                            "INSERT INTO release_group_album_link ({GROUP_LINK_COLUMNS}) \
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
                        ),
                        params![
                            group,
                            link.album.catalog.as_str(),
                            link.album.key,
                            columns.stated,
                            columns.wikidata_item,
                            columns.musicbrainz_release,
                            columns.release.map(|release| release.catalog.as_str()),
                            columns.release.map(|release| release.key.as_str()),
                        ],
                    )?;
                }
            }
            Ok(())
        })
        .await
    }
}
