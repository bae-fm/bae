//! The `source_release` tables: every catalog release bae fetched, as its
//! extraction left it.
//!
//! A release is written whole when it is fetched and read whole by every
//! import surface. Writing it again replaces every row under it in one
//! transaction, so a reader sees one extraction or the other, never a mix.

use super::*;
use crate::import::assemble::{ArtistRef, PartDirection};
use crate::import::cover_art::{CoverStanding, DownscaledCopy, RemoteCover, RemoteImageSet};
use crate::import::release_metadata::{AlbumMetadata, ReleaseMetadata};
use crate::import::source_release::{
    ArchiveRelease, ArtistCredit, CatalogFacts, EntryKind, PerformedWork, ReleaseCovers,
    RoleCredit, SourceMedium, SourceRelease, SourceWork, SourceWorkEvent, TracklistEntry,
    UnfetchedDocument, UnfetchedReason,
};
use crate::import::{MetadataRef, ReleaseRecord};
use crate::pressing::{Medium, Pressing, StatedFormat, StatedMedia};
use tracklist::{insert_entry, load_mediums, Numbering};

#[path = "source_releases/tracklist.rs"]
mod tracklist;

/// The tables under `source_release`, children before the tables they hang
/// off. Deleting a release's mediums takes its entries, and with them their
/// credits, roles and works.
const CHILD_TABLES: [&str; 10] = [
    "source_release_album_artist",
    "source_release_link",
    "source_release_format",
    "source_release_record",
    "source_release_cover_copy",
    "source_release_cover",
    "source_release_archive_group",
    "source_release_role",
    "source_release_medium",
    "source_release_unfetched",
];

fn unreadable(what: &str, value: impl std::fmt::Debug) -> DbError {
    DbError::Message(format!("stored source release holds an unreadable {what}: {value:?}"))
}

fn catalog_column(value: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(value).map_err(|_| unreadable("catalog", value))
}

fn to_u64(value: i64, what: &str) -> Result<u64, DbError> {
    u64::try_from(value).map_err(|_| unreadable(what, value))
}

fn to_i64(value: u64, what: &str) -> Result<i64, DbError> {
    i64::try_from(value)
        .map_err(|_| DbError::Message(format!("{what} {value} exceeds SQLite's integer range")))
}

/// Write `release`, replacing whatever was stored for it.
pub(super) fn replace_source_release_on(
    sql: &SqlContext<'_, '_>,
    release: &SourceRelease,
    fetched_at: DateTime<Utc>,
) -> Result<(), DbError> {
    let catalog = release.release.catalog.as_str();
    let key = release.release.key.as_str();
    let metadata = &release.metadata;
    let pressing = &metadata.pressing;
    let facts = super::pressing_columns::FactColumns::of(&pressing.facts);
    sql.execute(
        "INSERT INTO source_release \
             (catalog, release_id, source_group_id, album_title, album_year, year, \
              label, catalog_number, barcode, country, region, media, status, packaging, \
              discogs_details, archive_release_id, archive_group_id, fetched_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (catalog, release_id) DO UPDATE SET \
             source_group_id = excluded.source_group_id, \
             album_title = excluded.album_title, album_year = excluded.album_year, \
             year = excluded.year, label = excluded.label, \
             catalog_number = excluded.catalog_number, barcode = excluded.barcode, \
             country = excluded.country, region = excluded.region, media = excluded.media, \
             status = excluded.status, packaging = excluded.packaging, \
             discogs_details = excluded.discogs_details, \
             archive_release_id = excluded.archive_release_id, \
             archive_group_id = excluded.archive_group_id, fetched_at = excluded.fetched_at",
        params![
            catalog,
            key,
            release.source_group_id,
            metadata.album.title,
            metadata.album.year,
            pressing.year,
            pressing.label,
            pressing.catalog_number,
            pressing.barcode,
            facts.country,
            facts.region,
            facts.media,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            release
                .archive_release
                .as_ref()
                .map(|archive| archive.release_id.as_str()),
            release
                .archive_release
                .as_ref()
                .and_then(|archive| archive.group_id.as_deref()),
            fetched_at.to_rfc3339(),
        ],
    )?;
    for table in CHILD_TABLES {
        sql.execute(
            &format!("DELETE FROM {table} WHERE catalog = ? AND release_id = ?"),
            params![catalog, key],
        )?;
    }
    for (position, artist) in metadata.album.artists.iter().enumerate() {
        sql.execute(
            "INSERT INTO source_release_album_artist \
                 (catalog, release_id, position, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                catalog,
                key,
                position as i64,
                artist.name,
                artist.sort_name,
                artist.musicbrainz_artist_id,
                artist.discogs_artist_id,
            ],
        )?;
    }
    match &release.catalog {
        CatalogFacts::MusicBrainz { links } => {
            for (position, link) in links.iter().enumerate() {
                sql.execute(
                    "INSERT INTO source_release_link \
                         (catalog, release_id, position, link_catalog, link_key) \
                     VALUES (?, ?, ?, ?, ?)",
                    params![catalog, key, position as i64, link.catalog.as_str(), link.key],
                )?;
            }
        }
        CatalogFacts::Discogs {
            media,
            release_roles,
        } => {
            let formats: &[StatedFormat] = match media {
                StatedMedia::Formats(formats) => formats,
                StatedMedia::Undescribed => &[],
                StatedMedia::PerMedium(_) => {
                    unreachable!("a Discogs release states its media as format entries")
                }
            };
            for (position, format) in formats.iter().enumerate() {
                sql.execute(
                    "INSERT INTO source_release_format \
                         (catalog, release_id, position, medium, quantity) \
                     VALUES (?, ?, ?, ?, ?)",
                    params![
                        catalog,
                        key,
                        position as i64,
                        format.medium.map(Medium::key),
                        format.quantity,
                    ],
                )?;
            }
            for role in release_roles {
                sql.execute(
                    "INSERT INTO source_release_role \
                         (catalog, release_id, position, name, sort_name, \
                          musicbrainz_artist_id, discogs_artist_id, role) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        catalog,
                        key,
                        role.position,
                        role.artist.name,
                        role.artist.sort_name,
                        role.artist.musicbrainz_artist_id,
                        role.artist.discogs_artist_id,
                        role.role,
                    ],
                )?;
            }
        }
    }
    for record in &release.other_records {
        let (kind, album_key) = match record {
            ReleaseRecord::Pressing { album_key, .. } => ("pressing", album_key.as_deref()),
            ReleaseRecord::Album { .. } => ("album", None),
        };
        sql.execute(
            "INSERT INTO source_release_record \
                 (catalog, release_id, record_catalog, kind, key, album_key) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                catalog,
                key,
                record.catalog().as_str(),
                kind,
                record.key(),
                album_key
            ],
        )?;
    }
    for (scope, covers) in [
        ("release", &release.covers.release),
        ("album", &release.covers.album),
    ] {
        for (position, cover) in covers.iter().enumerate() {
            sql.execute(
                "INSERT INTO source_release_cover \
                     (catalog, release_id, scope, position, url, label, source, standing) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    catalog,
                    key,
                    scope,
                    position as i64,
                    cover.image.url,
                    cover.label,
                    cover.source.as_str(),
                    cover.standing.as_str(),
                ],
            )?;
            for copy in &cover.image.downscaled {
                sql.execute(
                    "INSERT INTO source_release_cover_copy \
                         (catalog, release_id, scope, position, max_edge, url) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                    params![catalog, key, scope, position as i64, copy.max_edge, copy.url],
                )?;
            }
        }
    }
    for (position, group) in release.archive_groups.iter().enumerate() {
        sql.execute(
            "INSERT INTO source_release_archive_group (catalog, release_id, position, group_id) \
             VALUES (?, ?, ?, ?)",
            params![catalog, key, position as i64, group],
        )?;
    }
    for unfetched in &release.unfetched {
        sql.execute(
            "INSERT INTO source_release_unfetched (catalog, release_id, document, key, reason) \
             VALUES (?, ?, ?, ?, ?)",
            params![
                catalog,
                key,
                unfetched.document.as_str(),
                unfetched.key,
                match unfetched.reason {
                    UnfetchedReason::Failed => "failed",
                    UnfetchedReason::DiscogsNotConfigured => "discogs_not_configured",
                },
            ],
        )?;
    }
    let mut numbering = Numbering::default();
    for (position, medium) in release.mediums.iter().enumerate() {
        sql.execute(
            "INSERT INTO source_release_medium (catalog, release_id, position, medium) \
             VALUES (?, ?, ?, ?)",
            params![catalog, key, position as i64, medium.medium.map(Medium::key)],
        )?;
        for entry in &medium.entries {
            insert_entry(sql, catalog, key, position as i64, None, entry, &mut numbering)?;
        }
    }
    Ok(())
}

fn artist_at(row: &Row<'_>, first: usize) -> coven::rusqlite::Result<ArtistRef> {
    Ok(ArtistRef {
        name: row.get(first)?,
        sort_name: row.get(first + 1)?,
        musicbrainz_artist_id: row.get(first + 2)?,
        discogs_artist_id: row.get(first + 3)?,
    })
}

/// The stored release `release` names, read whole, or `None` when nothing
/// has fetched it.
///
/// An album a kept statement joins to one of the release's own — one its
/// documents never reach — is derived here, on every read, from the statement
/// and the stored release together: its record, and for a MusicBrainz release
/// group the archive's images of it. Nothing about it is stored with the
/// release, so the release reads the same whether it was fetched before the
/// statement was read or after, and stops naming the album when a later
/// reading stops stating it. The album's own text — its title, credits and
/// year — lives in documents a statement does not carry, so a joined album
/// contributes none; the release's own album documents state those.
pub(super) fn load_source_release_on<S: QueryOne + QueryRows>(
    sql: &S,
    release: &MetadataRef,
) -> Result<Option<SourceRelease>, DbError> {
    let catalog = release.catalog.as_str();
    let key = release.key.as_str();
    let head = sql
        .query_row(
            "SELECT source_group_id, album_title, album_year, year, label, catalog_number, \
                    barcode, archive_release_id, archive_group_id, country, region, media, \
                    status, packaging, discogs_details \
             FROM source_release WHERE catalog = ? AND release_id = ?",
            params![catalog, key],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i32>>(2)?,
                    Pressing {
                        year: row.get(3)?,
                        label: row.get(4)?,
                        catalog_number: row.get(5)?,
                        barcode: row.get(6)?,
                        facts: super::pressing_columns::read_facts(row, "")?,
                    },
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((source_group_id, album_title, album_year, pressing, archive_release, archive_group)) =
        head
    else {
        return Ok(None);
    };
    let archive_release = match (archive_release, archive_group) {
        (Some(release_id), group_id) => Some(ArchiveRelease {
            release_id,
            group_id,
        }),
        (None, None) => None,
        (None, Some(group)) => return Err(unreadable("archive group without a release", group)),
    };
    let album_artists = sql.query(
        "SELECT name, sort_name, musicbrainz_artist_id, discogs_artist_id \
         FROM source_release_album_artist WHERE catalog = ? AND release_id = ? \
         ORDER BY position",
        params![catalog, key],
        |row| artist_at(row, 0),
    )?;
    let catalog_facts = match release.catalog {
        Catalog::MusicBrainz => CatalogFacts::MusicBrainz {
            links: sql
                .query(
                    "SELECT link_catalog, link_key FROM source_release_link \
                     WHERE catalog = ? AND release_id = ? ORDER BY position",
                    params![catalog, key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )?
                .into_iter()
                .map(|(link_catalog, link_key)| {
                    Ok(MetadataRef::new(catalog_column(&link_catalog)?, link_key))
                })
                .collect::<Result<_, DbError>>()?,
        },
        Catalog::Discogs => CatalogFacts::Discogs {
            media: {
                let formats: Vec<StatedFormat> = sql.query(
                    "SELECT medium, quantity FROM source_release_format \
                     WHERE catalog = ? AND release_id = ? ORDER BY position",
                    params![catalog, key],
                    |row| {
                        Ok(StatedFormat {
                            medium: super::pressing_columns::keyed(row, "medium", Medium::from_key)?,
                            quantity: row.get(1)?,
                        })
                    },
                )?;
                if formats.is_empty() {
                    StatedMedia::Undescribed
                } else {
                    StatedMedia::Formats(formats)
                }
            },
            release_roles: sql.query(
                "SELECT position, name, sort_name, musicbrainz_artist_id, discogs_artist_id, role \
                 FROM source_release_role WHERE catalog = ? AND release_id = ? \
                 ORDER BY position",
                params![catalog, key],
                |row| {
                    Ok(RoleCredit {
                        position: row.get(0)?,
                        artist: artist_at(row, 1)?,
                        role: row.get(5)?,
                    })
                },
            )?,
        },
        other => return Err(unreadable("catalog", other.as_str())),
    };
    let mut other_records: Vec<ReleaseRecord> = sql
        .query(
            "SELECT record_catalog, kind, key, album_key FROM source_release_record \
             WHERE catalog = ? AND release_id = ?",
            params![catalog, key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )?
        .into_iter()
        .map(|(record_catalog, kind, record_key, album_key)| {
            let reference = MetadataRef::new(catalog_column(&record_catalog)?, record_key);
            match kind.as_str() {
                "pressing" => Ok(ReleaseRecord::new(&reference, album_key, false)),
                "album" => Ok(ReleaseRecord::album(&reference)),
                other => Err(unreadable("record kind", other)),
            }
        })
        .collect::<Result<_, DbError>>()?;
    let joined = super::album_link_rows::joined_albums_on(
        sql,
        release.catalog,
        source_group_id.as_deref(),
        &other_records,
    )?;
    other_records.extend(joined.iter().map(ReleaseRecord::album));
    other_records.sort_by_key(|record| crate::import::source_release::catalog_rank(record.catalog()));
    let mut covers = ReleaseCovers {
        release: Vec::new(),
        album: Vec::new(),
    };
    let mut copies: HashMap<(String, i64), Vec<DownscaledCopy>> = HashMap::new();
    for (scope, position, max_edge, url) in sql.query(
        "SELECT scope, position, max_edge, url FROM source_release_cover_copy \
         WHERE catalog = ? AND release_id = ?",
        params![catalog, key],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )? {
        copies
            .entry((scope, position))
            .or_default()
            .push(DownscaledCopy { url, max_edge });
    }
    for (scope, position, url, label, source, standing) in sql.query(
        "SELECT scope, position, url, label, source, standing FROM source_release_cover \
         WHERE catalog = ? AND release_id = ? ORDER BY scope, position",
        params![catalog, key],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )? {
        let downscaled = copies
            .remove(&(scope.clone(), position))
            .unwrap_or_default();
        let cover = RemoteCover {
            image: RemoteImageSet::with_copies(url, downscaled),
            label,
            source: catalog_column(&source)?,
            standing: CoverStanding::from_column(&standing)
                .ok_or_else(|| unreadable("cover standing", &standing))?,
        };
        match scope.as_str() {
            "release" => covers.release.push(cover),
            "album" => covers.album.push(cover),
            other => return Err(unreadable("cover scope", other)),
        }
    }
    let mut archive_groups: Vec<String> = sql.query(
        "SELECT group_id FROM source_release_archive_group \
         WHERE catalog = ? AND release_id = ? ORDER BY position",
        params![catalog, key],
        |row| row.get(0),
    )?;
    // A joined release group's images, addressed by its id as the extraction
    // addresses a group its documents reach.
    for group in joined
        .iter()
        .filter(|album| album.catalog == Catalog::MusicBrainz)
    {
        crate::import::cover_art::push_unique_cover(
            &mut covers.album,
            RemoteCover::musicbrainz_release_group(&group.key),
        );
        if !archive_groups.contains(&group.key) {
            archive_groups.push(group.key.clone());
        }
    }
    let mediums = load_mediums(sql, catalog, key)?;
    let unfetched = sql
        .query(
            "SELECT document, key, reason FROM source_release_unfetched \
             WHERE catalog = ? AND release_id = ? ORDER BY document, key",
            params![catalog, key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?
        .into_iter()
        .map(|(document, document_key, reason)| {
            Ok(UnfetchedDocument {
                document: crate::import::PayloadSource::from_str(&document)
                    .map_err(|_| unreadable("unfetched document", &document))?,
                key: document_key,
                reason: match reason.as_str() {
                    "failed" => UnfetchedReason::Failed,
                    "discogs_not_configured" => UnfetchedReason::DiscogsNotConfigured,
                    other => return Err(unreadable("unfetched reason", other)),
                },
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(Some(SourceRelease {
        release: release.clone(),
        source_group_id,
        metadata: ReleaseMetadata {
            album: AlbumMetadata {
                title: album_title,
                artists: album_artists,
                year: album_year,
            },
            pressing,
        },
        other_records,
        covers,
        archive_release,
        archive_groups,
        mediums,
        catalog: catalog_facts,
        unfetched,
    }))
}

impl Database {
    /// The stored release `release` names, or `None` when nothing has
    /// fetched it.
    pub async fn load_source_release(
        &self,
        release: &MetadataRef,
    ) -> Result<Option<SourceRelease>, DbError> {
        let release = release.clone();
        self.read(move |sql| load_source_release_on(&sql, &release))
            .await
    }

    /// Store a fetched release, replacing whatever was stored for it.
    pub async fn save_source_release(&self, release: &SourceRelease) -> Result<(), DbError> {
        let release = release.clone();
        let fetched_at = self.now();
        self.call(move |sql| replace_source_release_on(sql, &release, fetched_at))
            .await
    }
}
