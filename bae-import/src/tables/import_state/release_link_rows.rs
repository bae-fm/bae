//! The candidate's release link: the pressing a pick or identification
//! linked it to, with that pressing's partners on other catalogs, or the
//! album a person linked it to, written in place when it changes and read
//! back with every read that shows the candidate.

use super::*;
use crate::ReleaseLink;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::MetadataRef;
use bae_identify::{AlbumLink, PressingLink};
use std::str::FromStr;

/// Make the candidate's release link `link`, or remove it. A link that
/// stays is written in place, so the candidate never reads as unlinked on
/// its way from one link to another.
pub(super) fn store_release_link(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    link: Option<&ReleaseLink>,
) -> Result<(), DbError> {
    let Some(link) = link else {
        // The partner and album rows hang off the link row, so this clears
        // them too.
        sql.execute(
            "DELETE FROM import_candidate_release_link WHERE content_hash = ?",
            [content_hash],
        )?;
        return Ok(());
    };
    // The partner and album rows name the link's kind, which the row below
    // may change, so the previous link's go first.
    sql.execute(
        "DELETE FROM import_candidate_release_link_partner WHERE content_hash = ?",
        [content_hash],
    )?;
    sql.execute(
        "DELETE FROM import_candidate_release_link_album WHERE content_hash = ?",
        [content_hash],
    )?;
    let (kind, source, release_id) = match link {
        ReleaseLink::Pressing(pressing) => {
            ("pressing", Some(pressing.record.catalog.as_str()), Some(pressing.record.key.as_str()))
        }
        ReleaseLink::Album(_) => ("album", None, None),
    };
    sql.execute(
        "INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (content_hash) DO UPDATE SET \
             kind = excluded.kind, source = excluded.source, release_id = excluded.release_id",
        params![content_hash, kind, source, release_id],
    )?;
    match link {
        ReleaseLink::Pressing(pressing) => {
            for partner in &pressing.partners {
                sql.execute(
                    "INSERT INTO import_candidate_release_link_partner \
                         (content_hash, kind, source, release_id) \
                     VALUES (?, 'pressing', ?, ?)",
                    params![content_hash, partner.catalog.as_str(), partner.key],
                )?;
            }
        }
        ReleaseLink::Album(album) => {
            for album in album.albums() {
                sql.execute(
                    "INSERT INTO import_candidate_release_link_album \
                         (content_hash, kind, catalog, album_id) \
                     VALUES (?, 'album', ?, ?)",
                    params![content_hash, album.catalog.as_str(), album.key],
                )?;
            }
        }
    }
    Ok(())
}

/// A candidate's release link as columns of a read that joins its row as
/// `link`: a pressing's partners in the other catalogs, by catalog, and an
/// album's album in each catalog, in catalog order, riding on it as JSON
/// arrays. A candidate with no link reads `link_row` as NULL.
pub(crate) const RELEASE_LINK_COLUMNS: &str = "link.content_hash AS link_row, \
     link.kind AS link_kind, link.source AS link_source, link.release_id AS link_release_id, \
     (SELECT json_group_array(json_array(partner.source, partner.release_id) \
                              ORDER BY partner.source) \
      FROM import_candidate_release_link_partner AS partner \
      WHERE partner.content_hash = link.content_hash) AS link_partners, \
     (SELECT json_group_array(json_array(album.catalog, album.album_id) \
                              ORDER BY bae_catalog_rank(album.catalog)) \
      FROM import_candidate_release_link_album AS album \
      WHERE album.content_hash = link.content_hash) AS link_albums";

/// One candidate's [`RELEASE_LINK_COLUMNS`] as SQLite hands them over, its
/// lists still JSON.
pub(crate) struct StoredReleaseLink {
    content_hash: String,
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
    partners: String,
    albums: String,
}

/// The [`RELEASE_LINK_COLUMNS`] of `row`; none for an unlinked candidate.
pub(crate) fn read_release_link(row: &Row<'_>) -> rusqlite::Result<Option<StoredReleaseLink>> {
    let Some(content_hash) = row.get::<_, Option<String>>("link_row")? else {
        return Ok(None);
    };
    Ok(Some(StoredReleaseLink {
        content_hash,
        kind: row.get("link_kind")?,
        source: row.get("link_source")?,
        release_id: row.get("link_release_id")?,
        partners: row.get("link_partners")?,
        albums: row.get("link_albums")?,
    }))
}

impl StoredReleaseLink {
    pub(crate) fn link(self) -> Result<ReleaseLink, DbError> {
        let Self { content_hash, kind, source, release_id, partners, albums } = self;
        let pairs = |json: &str, what: &str| {
            serde_json::from_str::<Vec<(String, String)>>(json).map_err(|error| {
                DbError::Message(format!("{content_hash}'s stored {what} {json:?}: {error}"))
            })
        };
        match (kind.as_str(), source, release_id) {
            ("pressing", Some(source), Some(release_id)) => {
                Ok(ReleaseLink::Pressing(PressingLink {
                    record: lookup_reference(source, release_id)?,
                    partners: pairs(&partners, "link partners")?
                        .into_iter()
                        .map(|(catalog, key)| lookup_reference(catalog, key))
                        .collect::<Result<_, DbError>>()?,
                }))
            }
            ("album", None, None) => Ok(ReleaseLink::Album(
                AlbumLink::new(
                    pairs(&albums, "linked albums")?
                        .into_iter()
                        .map(|(catalog, key)| reference(catalog, key))
                        .collect::<Result<Vec<_>, DbError>>()?,
                )
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "{content_hash} is linked to an album no catalog names"
                    ))
                })?,
            )),
            (other, _, _) => Err(DbError::unreadable("import_candidate_release_link.kind", other)),
        }
    }
}

fn reference(catalog: String, key: String) -> Result<MetadataRef, DbError> {
    Ok(MetadataRef::new(Catalog::from_str(&catalog).map_err(DbError::Message)?, key))
}
