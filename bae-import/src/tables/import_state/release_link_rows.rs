//! The candidate's release link: the pressing a pick or identification
//! linked it to, with that pressing's partners on other catalogs, or the
//! album a person linked it to, written in place when it changes and read
//! back with every read that shows the candidate.
//! Its tables are `release_link_rows.sql`, which every device's library holds.

use super::*;
use crate::ReleaseLink;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::MetadataRef;
use bae_identify::{AlbumLink, PressingLink};
use bae_store::WriteStatements as _;
use std::str::FromStr;

coven::write!(
    DeleteReleaseLink(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_release_link WHERE content_hash = :content_hash
"#
);

coven::write!(
    DeleteLinkPartners(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_release_link_partner WHERE content_hash = :content_hash
"#
);

coven::write!(
    DeleteLinkAlbums(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_release_link_album WHERE content_hash = :content_hash
"#
);

coven::write!(
    UpsertReleaseLink(
        content_hash: &'p str,
        kind: &'static str,
        source: Option<&'p str>,
        release_id: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id)
    VALUES (:content_hash, :kind, :source, :release_id)
    ON CONFLICT (content_hash) DO UPDATE SET
        kind = excluded.kind, source = excluded.source, release_id = excluded.release_id
"#
);

coven::write!(
    InsertLinkPartner(content_hash: &'p str, source: &'p str, release_id: &'p str),
    r#"
    INSERT INTO import_candidate_release_link_partner (content_hash, kind, source, release_id)
    VALUES (:content_hash, 'pressing', :source, :release_id)
"#
);

coven::write!(
    InsertLinkAlbum(content_hash: &'p str, catalog: &'p str, album_id: &'p str),
    r#"
    INSERT INTO import_candidate_release_link_album (content_hash, kind, catalog, album_id)
    VALUES (:content_hash, 'album', :catalog, :album_id)
"#
);

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
        sql.run(&DeleteReleaseLinkParams { content_hash })?;
        return Ok(());
    };
    // The partner and album rows name the link's kind, which the row below
    // may change, so the previous link's go first.
    sql.run(&DeleteLinkPartnersParams { content_hash })?;
    sql.run(&DeleteLinkAlbumsParams { content_hash })?;
    let (kind, source, release_id) = match link {
        ReleaseLink::Pressing(pressing) => {
            ("pressing", Some(pressing.record.catalog.as_str()), Some(pressing.record.key.as_str()))
        }
        ReleaseLink::Album(_) => ("album", None, None),
    };
    sql.run(&UpsertReleaseLinkParams { content_hash, kind, source, release_id })?;
    match link {
        ReleaseLink::Pressing(pressing) => {
            for partner in &pressing.partners {
                sql.run(&InsertLinkPartnerParams {
                    content_hash,
                    source: partner.catalog.as_str(),
                    release_id: &partner.key,
                })?;
            }
        }
        ReleaseLink::Album(album) => {
            for album in album.albums() {
                sql.run(&InsertLinkAlbumParams {
                    content_hash,
                    catalog: album.catalog.as_str(),
                    album_id: &album.key,
                })?;
            }
        }
    }
    Ok(())
}

// The release link of the candidate `content_hash`; no row for one linked to
// nothing.
coven::read!(
    writer LinkOfCandidate(content_hash: String),
    r#"
    SELECT link.*
    FROM @crate::tables::import_state::listed_fragments::ListedLink AS link
    WHERE link.link_hash = :content_hash
"#
);

/// The release link of the candidate `content_hash` as
/// [`super::listed_fragments::ListedLink`] gives it; `None` for one linked to
/// nothing.
pub(super) fn load_release_link_on(
    sql: &impl bae_store::WriterRead,
    content_hash: &str,
) -> Result<Option<super::listed_fragments::ListedLink>, DbError> {
    let request = LinkOfCandidateParams { content_hash: content_hash.to_string() };
    Ok(bae_store::statement_optional(sql, &request)?.map(|row| row.link))
}

/// One candidate's release link as SQLite hands it over, its lists still
/// JSON.
#[derive(Clone, PartialEq)]
pub(crate) struct StoredReleaseLink {
    content_hash: String,
    kind: String,
    source: Option<String>,
    release_id: Option<String>,
    partners: String,
    albums: String,
}

impl StoredReleaseLink {
    /// The columns of a stored link, its lists as JSON arrays.
    pub(crate) fn new(
        content_hash: String,
        kind: String,
        source: Option<String>,
        release_id: Option<String>,
        partners: String,
        albums: String,
    ) -> Self {
        Self { content_hash, kind, source, release_id, partners, albums }
    }

    /// The link a pane's read selects, as
    /// [`super::state_fragments::CurrentState`] names it; none for an unlinked
    /// candidate.
    pub(crate) fn of(
        state: &super::state_fragments::CurrentState,
    ) -> Result<Option<Self>, DbError> {
        let Some(content_hash) = state.link_row.clone() else {
            return Ok(None);
        };
        Ok(Some(Self {
            content_hash,
            kind: super::required(state.link_kind.clone(), "link_kind")?,
            source: state.link_source.clone(),
            release_id: state.link_release_id.clone(),
            partners: state.link_partners.clone(),
            albums: state.link_albums.clone(),
        }))
    }
}

impl StoredReleaseLink {
    pub(crate) fn link(self) -> Result<ReleaseLink, DbError> {
        let Self { content_hash, kind, source, release_id, partners, albums } = self;
        let pairs = |json: &str, what: &str| {
            serde_json::from_str::<Vec<(String, String)>>(json).map_err(|error| {
                DbError::unreadable_json(format!("{content_hash}'s stored {what}"), json, error)
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
                    DbError::unreadable(
                        format!("the linked albums of {content_hash}'s release link"),
                        &albums,
                    )
                })?,
            )),
            (other, _, _) => Err(DbError::unreadable("import_candidate_release_link.kind", other)),
        }
    }
}

fn reference(catalog: String, key: String) -> Result<MetadataRef, DbError> {
    Ok(MetadataRef::new(
        Catalog::from_str(&catalog)
            .map_err(|_unknown| DbError::unreadable("a stored link's catalog", &catalog))?,
        key,
    ))
}
