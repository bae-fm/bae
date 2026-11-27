//! The `source_release` tables: every catalog release a row names — a match,
//! a draft read from it, a link or a linked pressing's partner — as its
//! extraction left it.
//!
//! A fetched release is held in memory until the write that writes the first
//! row naming it, which stores it whole (`store_named_releases_on`); the
//! schema's triggers delete it in the write that drops the last such row. It
//! is read whole by every import surface but the import list's rows, which
//! read only the records it names. Writing it again (`writes.rs`) replaces
//! every row under it in one transaction, so a reader sees one extraction or
//! the other, never a mix.
//!
//! Their tables are `source_releases.sql`, `source_releases/tracklist.sql` and
//! `album_link_rows.sql`.

use super::*;
use bae_catalogs::album_links::GroupAlbum;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::{MetadataRef, ReleaseRecord};
use bae_catalogs::release_metadata::{AlbumMetadata, ReleaseMetadata};
use bae_catalogs::remote_image::{CoverStanding, RemoteCover, RemoteImageSet};
use bae_catalogs::source_release::{
    ArchiveRelease, CatalogFacts, DisplayCredit, EntryKind, PartDirection, PerformedWork,
    PreparedRelease, ReleaseCovers, RoleCredit, SourceMedium, SourceRelease, SourceWork,
    SourceWorkEvent, TracklistEntry, UnfetchedDocument, UnfetchedReason,
};
use bae_store::statement_rows;
use bae_types::pressing::{Pressing, ReleaseLabel, StatedFormat, StatedMedia};
use reads::ReleasesWithKeptAlbumsParams;
use tracklist::{mediums_of, release_roles_of};
use writes::replace_source_release_on;

mod fragments;
mod reads;
pub mod row_fragments;
mod tracklist;
mod writes;

#[expect(
    clippy::map_err_ignore,
    reason = "the unreadable value is named; the parse says nothing past it"
)]
fn catalog_column(value: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(value)
        .map_err(|_| DbError::unreadable("a source release's stored catalog", value))
}

/// The album year a stored release reads back with: its album's first year,
/// else its pressing's year.
fn derived_album_year(first_year: Option<i32>, pressing_year: Option<i32>) -> Option<i32> {
    first_year.or(pressing_year)
}

/// Panic on a release whose album year or media counts differ from what its
/// other facts give, which is all the store keeps of them: an extraction
/// derives both that way, and storing one that doesn't would read back as
/// another release.
fn check_derived_facts(release: &SourceRelease) {
    let metadata = &release.metadata;
    let derived = derived_album_year(metadata.album.first_year, metadata.pressing.typed.year);
    assert!(
        metadata.album.year == derived,
        "{:?} states album year {:?}, not its first year or pressing year {derived:?}",
        release.release,
        metadata.album.year
    );
    let counts = release.stated_media().counts();
    assert!(
        metadata.pressing.facts.media == counts,
        "{:?} counts media {:?}, not the {counts:?} its mediums and formats state",
        release.release,
        metadata.pressing.facts.media
    );
}

/// The release `release` names as the columns `stored` read hold it: what its
/// own documents state, which is what a write that names it stores.
fn stored_release_of(
    release: &bae_catalogs::records::LookupRef,
    stored: fragments::StoredRelease,
) -> Result<SourceRelease, DbError> {
    let archive_release = match (stored.archive_release_id, stored.archive_group_id) {
        (Some(release_id), group_id) => Some(ArchiveRelease { release_id, group_id }),
        (None, None) => None,
        (None, Some(group)) => {
            return Err(DbError::unreadable(
                "a source release's archive group without a release",
                group,
            ))
        }
    };
    let facts = bae_records::records::pressing_columns::facts_without_media_of(
        stored.country,
        stored.region,
        stored.status.as_deref(),
        stored.packaging.as_deref(),
        stored.discogs_details,
    )
    .map_err(|detail| DbError::unreadable("a source release's pressing facts", detail))?;
    let labels = stored
        .labels
        .labels
        .into_iter()
        .map(|label| {
            ReleaseLabel::new(label.name.clone(), label.catalog_number.clone())
                .map(|read| read.with_ids(label.musicbrainz_label_id, label.discogs_label_id))
                .ok_or_else(|| {
                    DbError::unreadable("source_release_label", (label.name, label.catalog_number))
                })
        })
        .collect::<Result<_, DbError>>()?;
    // The media counts are the mediums' and formats', counted below.
    let mut pressing = Pressing {
        typed: bae_types::pressing::TypedPressing {
            year: stored.year,
            labels,
            barcodes: stored.barcodes.barcodes,
        },
        facts,
    };
    let album_artists = stored
        .album_artists
        .album_artists
        .into_iter()
        .map(|credit| bae_catalogs::credits::SourceCredit {
            credit: credit.artist.artist(),
            join: credit.join_phrase,
        })
        .collect();
    let catalog_facts = match release.catalog {
        LookupCatalog::MusicBrainz => CatalogFacts::MusicBrainz {
            links: stored
                .links
                .links
                .into_iter()
                .map(|link| Ok(MetadataRef::new(catalog_column(&link.catalog)?, link.key)))
                .collect::<Result<_, DbError>>()?,
            album_links: {
                let owner = format!("source release {} {}", release.catalog.as_str(), release.key);
                let Some(column) = stored.album_links else {
                    return Err(DbError::unreadable(
                        format!("the album links of {owner}"),
                        None::<String>,
                    ));
                };
                super::album_link_rows::album_links_of(
                    &owner,
                    &column,
                    stored.album_link_rows.album_link_rows,
                )?
            },
        },
        LookupCatalog::Discogs => CatalogFacts::Discogs {
            media: {
                let formats: Vec<StatedFormat> = stored
                    .formats
                    .formats
                    .into_iter()
                    .map(|format| {
                        Ok(StatedFormat {
                            medium: super::medium_of(
                                "source_release_format.medium",
                                format.medium,
                            )?,
                            quantity: format.quantity,
                        })
                    })
                    .collect::<Result<_, DbError>>()?;
                if formats.is_empty() {
                    StatedMedia::Undescribed
                } else {
                    StatedMedia::Formats(formats)
                }
            },
        },
    };
    let other_records = stored
        .records
        .records
        .into_iter()
        .map(|record| {
            let reference = MetadataRef::new(catalog_column(&record.catalog)?, record.key);
            match record.kind.as_str() {
                "pressing" => Ok(ReleaseRecord::new(&reference, record.album_key)),
                "album" => Ok(ReleaseRecord::album(&reference)),
                other => Err(DbError::unreadable("source_release_record.kind", other)),
            }
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let mut covers = ReleaseCovers { release: Vec::new(), album: Vec::new() };
    for stored_cover in stored.covers.covers {
        let cover = RemoteCover {
            image: RemoteImageSet::stored(stored_cover.url, stored_cover.copies),
            label: super::cover_labels::label_of(&stored_cover.label)?,
            source: catalog_column(&stored_cover.source)?,
            standing: CoverStanding::from_column(&stored_cover.standing).ok_or_else(|| {
                DbError::unreadable("source_release_cover.standing", &stored_cover.standing)
            })?,
        };
        match stored_cover.scope.as_str() {
            "release" => covers.release.push(cover),
            "album" => covers.album.push(cover),
            other => return Err(DbError::unreadable("source_release_cover.scope", other)),
        }
    }
    let mediums = mediums_of(stored.mediums.mediums, stored.entries.entries, stored.works.works)?;
    pressing.facts.media = catalog_facts.stated_media(&mediums).counts();
    let unfetched = stored
        .unfetched
        .unfetched
        .into_iter()
        .map(|missing| {
            Ok(UnfetchedDocument {
                document: bae_catalogs::records::PayloadSource::from_str(&missing.document)
                    .map_err(|error| {
                        DbError::unreadable(
                            format!("source_release_unfetched.document ({error})"),
                            &missing.document,
                        )
                    })?,
                key: missing.key,
                reason: match missing.reason.as_str() {
                    "failed" => UnfetchedReason::Failed,
                    "not_asked" => UnfetchedReason::NotAsked,
                    "stopped_short" => UnfetchedReason::StoppedShort,
                    other => {
                        return Err(DbError::unreadable("source_release_unfetched.reason", other))
                    }
                },
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(SourceRelease {
        release: release.clone(),
        source_group_id: stored.source_group_id,
        metadata: ReleaseMetadata {
            album: AlbumMetadata {
                title: stored.album_title,
                artists: album_artists,
                year: derived_album_year(stored.album_first_year, pressing.typed.year),
                first_year: stored.album_first_year,
            },
            pressing,
        },
        other_records,
        covers,
        archive_release,
        archive_groups: stored.archive_groups.archive_groups,
        mediums,
        release_roles: release_roles_of(stored.release_roles.release_roles)?,
        catalog: catalog_facts,
        notes: stored.notes,
        unfetched,
    })
}

/// `releases` as a JSON array of `[catalog, key]` pairs, in their order, for
/// a statement to read them by with `json_each`.
fn releases_json(releases: &[&bae_catalogs::records::LookupRef]) -> String {
    serde_json::Value::Array(
        releases
            .iter()
            .map(|release| serde_json::json!([release.catalog.as_str(), release.key]))
            .collect(),
    )
    .to_string()
}

/// `own` and the `joined` albums, in the order surfaces list catalogs.
fn with_joined_records(mut own: Vec<ReleaseRecord>, joined: &[MetadataRef]) -> Vec<ReleaseRecord> {
    own.extend(joined.iter().map(ReleaseRecord::album));
    own.sort_by_key(|record| bae_catalogs::source_release::catalog_rank(record.catalog()));
    own
}

/// `stored` beside how import surfaces read it: with every album that `kept`,
/// the group albums kept for its own albums, joins to one of them — one its
/// documents never reach.
///
/// The joined album is derived on every read, from the statement and the
/// release together: its record, and for a MusicBrainz release group the
/// archive's images of it. Nothing about it is stored with the release, so
/// the release reads the same whether it was fetched before the statement was
/// read or after, held in memory or stored, and stops naming the album when a
/// later reading stops stating it. The album's own text — its title, credits
/// and year — lives in documents a statement does not carry, so a joined
/// album contributes none; the release's own album documents state those.
fn prepared(stored: SourceRelease, kept: Vec<GroupAlbum>) -> PreparedRelease {
    let joined = super::album_link_rows::joined_albums(&stored, &kept);
    let mut release = stored.clone();
    release.other_records = with_joined_records(release.other_records, &joined);
    // A joined release group's images, addressed by its id as the extraction
    // addresses a group its documents reach.
    for group in joined.iter().filter(|album| album.catalog == Catalog::MusicBrainz) {
        bae_catalogs::remote_image::push_unique_cover(
            &mut release.covers.album,
            RemoteCover::musicbrainz_release_group(&group.key),
        );
        if !release.archive_groups.contains(&group.key) {
            release.archive_groups.push(group.key.clone());
        }
    }
    PreparedRelease { stored, release }
}

/// The stored release each of `releases` names, as its rows hold it beside
/// how import surfaces read it, in their order and in one statement; `None`
/// for one no row names.
fn load_prepared_releases_on<S: bae_store::WriterRead>(
    sql: &S,
    releases: &[&bae_catalogs::records::LookupRef],
) -> Result<Vec<Option<PreparedRelease>>, DbError> {
    let request = ReleasesWithKeptAlbumsParams { releases: releases_json(releases) };
    let mut read = vec![None; releases.len()];
    for row in statement_rows(sql, &request)? {
        let (at, release) = row
            .at
            .and_then(|at| usize::try_from(at).ok())
            .and_then(|at| releases.get(at).map(|release| (at, *release)))
            .ok_or_else(|| DbError::unreadable("a stored release's place", row.at))?;
        let stored = stored_release_of(release, row.release)?;
        let kept = super::album_link_rows::group_albums_of(row.kept)?;
        read[at] = Some(prepared(stored, kept));
    }
    Ok(read)
}

/// The stored release each of `releases` names, as import surfaces read it,
/// in their order and in one statement; `None` for one no row names.
pub(crate) fn load_source_releases_on<S: bae_store::WriterRead>(
    sql: &S,
    releases: &[&bae_catalogs::records::LookupRef],
) -> Result<Vec<Option<SourceRelease>>, DbError> {
    Ok(load_prepared_releases_on(sql, releases)?
        .into_iter()
        .map(|read| read.map(|read| read.release))
        .collect())
}

/// Store each of `releases` that a row names, in the write that wrote the
/// row: insert one not stored yet, and replace a stored one only where this
/// copy read a document the stored one lists as unfetched — a copy read
/// before the stored one was would otherwise overwrite a fuller read. A
/// release no row names is not stored: a catalog release is stored only while
/// something names it. What each link claiming a release this stores claims
/// is stored again from it.
pub(crate) fn store_named_releases_on(
    sql: &CovenWrite<'_>,
    releases: &[SourceRelease],
) -> Result<(), DbError> {
    let mut stored_now = Vec::new();
    for release in releases {
        let catalog = release.release.catalog.as_str();
        let key = release.release.key.as_str();
        let named = sql.query_row(
            "SELECT EXISTS (SELECT 1 FROM import_candidate_match \
                            WHERE source = ?1 AND release_id = ?2) \
                 OR EXISTS (SELECT 1 FROM import_candidate_draft_provenance \
                            WHERE source = ?1 AND release_id = ?2) \
                 OR EXISTS (SELECT 1 FROM import_candidate_release_link \
                            WHERE source = ?1 AND release_id = ?2) \
                 OR EXISTS (SELECT 1 FROM import_candidate_release_link_partner \
                            WHERE source = ?1 AND release_id = ?2)",
            params![catalog, key],
            |row| row.get::<_, bool>(0),
        )?;
        if !named {
            continue;
        }
        let stored = sql.query_row(
            "SELECT EXISTS (SELECT 1 FROM source_release WHERE catalog = ? AND release_id = ?)",
            params![catalog, key],
            |row| row.get::<_, bool>(0),
        )?;
        let reads_more = || -> Result<bool, DbError> {
            let stored_unfetched = sql.query(
                "SELECT document, key FROM source_release_unfetched \
                 WHERE catalog = ? AND release_id = ?",
                params![catalog, key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            Ok(stored_unfetched.iter().any(|(document, document_key)| {
                !release.unfetched.iter().any(|unfetched| {
                    unfetched.document.as_str() == document && unfetched.key == *document_key
                })
            }))
        };
        if !stored || reads_more()? {
            replace_source_release_on(sql, release)?;
            stored_now.push(&release.release);
        }
    }
    // Once every release is stored, since a link claims several.
    super::import_state::store_claims_of_releases_on(sql, &stored_now)
}

impl ImportTables {
    /// `load_source_releases_on` as its own read.
    pub async fn load_source_releases(
        &self,
        releases: &[&bae_catalogs::records::LookupRef],
    ) -> Result<Vec<Option<SourceRelease>>, DbError> {
        let releases: Vec<_> = releases.iter().copied().cloned().collect();
        self.store
            .read(move |sql| load_source_releases_on(&sql, &releases.iter().collect::<Vec<_>>()))
            .await
    }

    /// The stored release `release` names, as its rows hold it beside how
    /// import surfaces read it, or `None` when no row names it.
    pub(crate) async fn load_prepared_release(
        &self,
        release: &bae_catalogs::records::LookupRef,
    ) -> Result<Option<PreparedRelease>, DbError> {
        let release = release.clone();
        self.store
            .read(move |sql| {
                Ok(load_prepared_releases_on(&sql, &[&release])?
                    .pop()
                    .expect("one release has one entry"))
            })
            .await
    }

    /// `stored`, a release just fetched, beside how import surfaces read it.
    pub(crate) async fn prepared_release(
        &self,
        stored: SourceRelease,
    ) -> Result<PreparedRelease, DbError> {
        let albums = super::album_link_rows::own_albums(&stored);
        self.store
            .read(move |sql| {
                let kept = super::album_link_rows::kept_group_albums_on(&sql, &albums)?;
                Ok(prepared(stored, kept))
            })
            .await
    }

    /// Store `release` with no row naming it, which only a test's setup
    /// does: every other write stores a release in the write that names it.
    /// Panics when a link claims the release: what the link claims is stored
    /// from it, and only the writes that store it again keep that current.
    #[cfg(test)]
    pub async fn save_source_release(&self, release: &SourceRelease) -> Result<(), DbError> {
        let release = release.clone();
        self.store
            .call(move |sql| {
                let catalog = release.release.catalog.as_str();
                let key = release.release.key.as_str();
                let claimed = sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM import_candidate_release_link \
                                    WHERE source = ?1 AND release_id = ?2) \
                         OR EXISTS (SELECT 1 FROM import_candidate_release_link_partner \
                                    WHERE source = ?1 AND release_id = ?2)",
                    params![catalog, key],
                    |row| row.get::<_, bool>(0),
                )?;
                assert!(!claimed, "{:?} is claimed by a link", release.release);
                replace_source_release_on(sql, &release)
            })
            .await
    }
}

#[cfg(test)]
#[path = "source_releases_tests.rs"]
mod tests;
