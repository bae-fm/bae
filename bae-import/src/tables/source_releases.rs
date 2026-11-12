//! The `source_release` tables: every catalog release a row names — a match,
//! a draft read from it, a link or a linked pressing's partner — as its
//! extraction left it.
//!
//! A fetched release is held in memory until the write that writes the first
//! row naming it, which stores it whole ([`store_named_releases_on`]); the
//! schema's triggers delete it in the write that drops the last such row. It
//! is read whole by every import surface but the import list's rows, which
//! read only the records it names. Writing it again replaces every row under
//! it in one transaction, so a reader sees one extraction or the other, never
//! a mix.
//!
//! Their tables are `source_releases.sql`, `source_releases/tracklist.sql` and
//! `album_link_rows.sql`.

/// The source releases' part of the library's schema. The release tables come
/// first; the triggers that delete a release when no row names it fire on
/// candidate state's tables, so the part runs after candidate state's.
pub(super) const SCHEMA: bae_store::SchemaPart = bae_store::SchemaPart::new(&[
    include_str!("source_releases.sql"),
    tracklist::SQL,
    super::album_link_rows::SQL,
]);

use super::*;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::{MetadataRef, ReleaseRecord};
use bae_catalogs::release_metadata::{AlbumMetadata, ReleaseMetadata};
use bae_catalogs::remote_image::{CoverStanding, RemoteCover, RemoteImageSet};
use bae_catalogs::source_release::{
    ArchiveRelease, CatalogFacts, DisplayCredit, EntryKind, PartDirection, PerformedWork,
    PreparedRelease, ReleaseCovers, RoleCredit, SourceMedium, SourceRelease, SourceWork,
    SourceWorkEvent, TracklistEntry, UnfetchedDocument, UnfetchedReason,
};
use bae_types::pressing::{Medium, Pressing, ReleaseLabel, StatedFormat, StatedMedia};
use tracklist::{insert_entry, load_mediums, Numbering};

mod tracklist;

/// The tables under `source_release`, children before the tables they hang
/// off. Deleting a release's mediums takes its entries, and with them their
/// credits, roles and works.
const CHILD_TABLES: [&str; 13] = [
    "source_release_label",
    "source_release_barcode",
    "source_release_album_artist",
    "source_release_link",
    "source_release_album_link",
    "source_release_format",
    "source_release_record",
    "source_release_cover_copy",
    "source_release_cover",
    "source_release_archive_group",
    "source_release_role",
    "source_release_medium",
    "source_release_unfetched",
];

/// The stored `album_links` values of a MusicBrainz release.
const ALBUM_LINKS_READ: &str = "read";
const ALBUM_LINKS_UNREAD: &str = "unread";

/// The `album_links` column a release's facts are stored with: a MusicBrainz
/// release's reading, none for a Discogs release. Fetching a MusicBrainz
/// release reads its album links, or fails to, so a stored one never holds
/// links nothing asked for.
fn album_links_column(facts: &CatalogFacts) -> Option<&'static str> {
    match facts {
        CatalogFacts::MusicBrainz { album_links, .. } => match album_links {
            AlbumLinks::Read(_) => Some(ALBUM_LINKS_READ),
            AlbumLinks::Unread => Some(ALBUM_LINKS_UNREAD),
            AlbumLinks::NotAsked => {
                panic!("a fetched MusicBrainz release carries album links that were never read")
            }
        },
        CatalogFacts::Discogs { .. } => None,
    }
}

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
    let derived = derived_album_year(metadata.album.first_year, metadata.pressing.year);
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

/// Write `release`, replacing whatever was stored for it.
pub(super) fn replace_source_release_on(
    sql: &CovenWrite<'_>,
    release: &SourceRelease,
) -> Result<(), DbError> {
    check_derived_facts(release);
    let catalog = release.release.catalog.as_str();
    let key = release.release.key.as_str();
    let metadata = &release.metadata;
    let pressing = &metadata.pressing;
    let facts = bae_records::records::pressing_columns::FactColumns::of(&pressing.facts);
    sql.execute(
        "INSERT INTO source_release \
             (catalog, release_id, source_group_id, album_title, \
              album_first_year, year, country, region, status, \
              packaging, discogs_details, notes, archive_release_id, archive_group_id, \
              album_links) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (catalog, release_id) DO UPDATE SET \
             source_group_id = excluded.source_group_id, \
             album_title = excluded.album_title, \
             album_first_year = excluded.album_first_year, \
             year = excluded.year, \
             country = excluded.country, region = excluded.region, \
             status = excluded.status, packaging = excluded.packaging, \
             discogs_details = excluded.discogs_details, notes = excluded.notes, \
             archive_release_id = excluded.archive_release_id, \
             archive_group_id = excluded.archive_group_id, \
             album_links = excluded.album_links",
        params![
            catalog,
            key,
            release.source_group_id,
            metadata.album.title,
            metadata.album.first_year,
            pressing.year,
            facts.country,
            facts.region,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            serde_json::to_string(&release.notes).expect("notes serialize"),
            release.archive_release.as_ref().map(|archive| archive.release_id.as_str()),
            release.archive_release.as_ref().and_then(|archive| archive.group_id.as_deref()),
            album_links_column(&release.catalog),
        ],
    )?;
    for table in CHILD_TABLES {
        sql.execute(
            &format!("DELETE FROM {table} WHERE catalog = ? AND release_id = ?"),
            params![catalog, key],
        )?;
    }
    for (position, label) in pressing.labels.iter().enumerate() {
        sql.execute(
            "INSERT INTO source_release_label \
                 (catalog, release_id, position, name, catalog_number, \
                  musicbrainz_label_id, discogs_label_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                catalog,
                key,
                position as i64,
                label.name(),
                label.catalog_number(),
                label.musicbrainz_label_id(),
                label.discogs_label_id()
            ],
        )?;
    }
    for (position, barcode) in pressing.barcodes.iter().enumerate() {
        sql.execute(
            "INSERT INTO source_release_barcode (catalog, release_id, position, barcode) \
             VALUES (?, ?, ?, ?)",
            params![catalog, key, position as i64, barcode],
        )?;
    }
    for (position, stated) in metadata.album.artists.iter().enumerate() {
        let artist = &stated.credit;
        sql.execute(
            "INSERT INTO source_release_album_artist \
                 (catalog, release_id, position, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id, credited_as, join_phrase) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                catalog,
                key,
                position as i64,
                artist.name,
                artist.sort_name,
                artist.musicbrainz_artist_id,
                artist.discogs_artist_id,
                artist.credited_as,
                stated.join,
            ],
        )?;
    }
    match &release.catalog {
        CatalogFacts::MusicBrainz { links, album_links } => {
            for (position, link) in links.iter().enumerate() {
                sql.execute(
                    "INSERT INTO source_release_link \
                         (catalog, release_id, position, link_catalog, link_key) \
                     VALUES (?, ?, ?, ?, ?)",
                    params![catalog, key, position as i64, link.catalog.as_str(), link.key],
                )?;
            }
            for (ordinal, album) in album_links.read().iter().enumerate() {
                sql.execute(
                    "INSERT INTO source_release_album_link \
                         (catalog, release_id, ordinal, album_catalog, album_key) \
                     VALUES (?, ?, ?, ?, ?)",
                    params![catalog, key, ordinal as i64, album.catalog.as_str(), album.key,],
                )?;
            }
        }
        CatalogFacts::Discogs { media, release_roles } => {
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
            params![catalog, key, record.catalog().as_str(), kind, record.key(), album_key],
        )?;
    }
    for (scope, covers) in [("release", &release.covers.release), ("album", &release.covers.album)]
    {
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
                    super::cover_labels::label_json(&cover.label)?,
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
                    UnfetchedReason::NotAsked => "not_asked",
                    UnfetchedReason::StoppedShort => "stopped_short",
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
        for (disc, disc_id) in medium.disc_ids.iter().enumerate() {
            sql.execute(
                "INSERT INTO source_release_medium_disc \
                 (catalog, release_id, position, disc, disc_id) VALUES (?, ?, ?, ?, ?)",
                params![catalog, key, position as i64, disc as i64, disc_id],
            )?;
        }
        for entry in &medium.entries {
            insert_entry(sql, catalog, key, position as i64, None, entry, &mut numbering)?;
        }
    }
    Ok(())
}

fn artist_at(row: &Row<'_>, first: usize) -> rusqlite::Result<bae_catalogs::credits::ArtistCredit> {
    Ok(bae_catalogs::credits::ArtistCredit {
        name: row.get(first)?,
        sort_name: row.get(first + 1)?,
        musicbrainz_artist_id: row.get(first + 2)?,
        discogs_artist_id: row.get(first + 3)?,
        credited_as: None,
    })
}

/// The stored release `release` names as its rows hold it — what its own
/// documents state, which is what a write that names it stores — or `None`
/// when no row names it.
fn load_source_release_rows_on<S: coven::SqlRead>(
    sql: &S,
    release: &bae_catalogs::records::LookupRef,
) -> Result<Option<SourceRelease>, DbError> {
    let catalog = release.catalog.as_str();
    let key = release.key.as_str();
    let head = sql
        .query_row(
            "SELECT source_group_id, album_title, year, \
                    archive_release_id, archive_group_id, country, region, \
                    status, packaging, discogs_details, album_first_year, notes, album_links \
             FROM source_release WHERE catalog = ? AND release_id = ?",
            params![catalog, key],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    // The media counts are the mediums' and formats', read
                    // below.
                    Pressing {
                        year: row.get(2)?,
                        // Read from their rows below.
                        labels: Vec::new(),
                        barcodes: Vec::new(),
                        facts: bae_records::records::pressing_columns::read_facts_without_media(
                            row, "",
                        )?,
                    },
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i32>>("album_first_year")?,
                    row.get::<_, String>("notes")?,
                    row.get::<_, Option<String>>("album_links")?,
                ))
            },
        )
        .optional()?;
    let Some((
        source_group_id,
        album_title,
        mut pressing,
        archive_release,
        archive_group,
        album_first_year,
        notes,
        album_links,
    )) = head
    else {
        return Ok(None);
    };
    let archive_release = match (archive_release, archive_group) {
        (Some(release_id), group_id) => Some(ArchiveRelease { release_id, group_id }),
        (None, None) => None,
        (None, Some(group)) => {
            return Err(DbError::unreadable(
                "a source release's archive group without a release",
                group,
            ))
        }
    };
    let notes: Vec<String> = serde_json::from_str(&notes)
        .map_err(|error| DbError::unreadable("source_release.notes", (&notes, error)))?;
    pressing.labels = sql
        .query(
            "SELECT name, catalog_number, musicbrainz_label_id, discogs_label_id \
             FROM source_release_label \
             WHERE catalog = ? AND release_id = ? ORDER BY position",
            params![catalog, key],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )?
        .into_iter()
        .map(|(name, catalog_number, musicbrainz, discogs)| {
            ReleaseLabel::new(name.clone(), catalog_number.clone())
                .map(|label| label.with_ids(musicbrainz, discogs))
                .ok_or_else(|| {
                    DbError::unreadable("source_release_cover.label", (name, catalog_number))
                })
        })
        .collect::<Result<_, DbError>>()?;
    pressing.barcodes = sql.query(
        "SELECT barcode FROM source_release_barcode \
         WHERE catalog = ? AND release_id = ? ORDER BY position",
        params![catalog, key],
        |row| row.get(0),
    )?;
    let album_artists = sql.query(
        "SELECT name, sort_name, musicbrainz_artist_id, discogs_artist_id, credited_as, \
                join_phrase \
         FROM source_release_album_artist WHERE catalog = ? AND release_id = ? \
         ORDER BY position",
        params![catalog, key],
        |row| {
            Ok(bae_catalogs::credits::SourceCredit {
                credit: bae_catalogs::credits::ArtistCredit {
                    credited_as: row.get(4)?,
                    ..artist_at(row, 0)?
                },
                join: row.get(5)?,
            })
        },
    )?;
    let catalog_facts = match release.catalog {
        LookupCatalog::MusicBrainz => CatalogFacts::MusicBrainz {
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
            album_links: match album_links.as_deref() {
                Some(ALBUM_LINKS_READ) => AlbumLinks::Read(
                    sql.query(
                        "SELECT album_catalog, album_key FROM source_release_album_link \
                         WHERE catalog = ? AND release_id = ? ORDER BY ordinal",
                        params![catalog, key],
                        |row| {
                            Ok(super::album_link_rows::AlbumLinkRow {
                                catalog: row.get(0)?,
                                key: row.get(1)?,
                            })
                        },
                    )?
                    .into_iter()
                    .map(super::album_link_rows::AlbumLinkRow::album)
                    .collect::<Result<_, DbError>>()?,
                ),
                Some(ALBUM_LINKS_UNREAD) => AlbumLinks::Unread,
                other => return Err(DbError::unreadable("source_release.album_links", other)),
            },
        },
        LookupCatalog::Discogs => CatalogFacts::Discogs {
            media: {
                let formats: Vec<StatedFormat> = sql.query(
                    "SELECT medium, quantity FROM source_release_format \
                     WHERE catalog = ? AND release_id = ? ORDER BY position",
                    params![catalog, key],
                    |row| {
                        Ok(StatedFormat {
                            medium: bae_records::records::pressing_columns::keyed(
                                row,
                                "medium",
                                Medium::from_key,
                            )?,
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
    };
    // The record rows keep no order: they read in the order surfaces list
    // catalogs.
    let own = own_records_of_on(sql, &[release])?.pop().expect("one release has one entry");
    let other_records = with_joined_records(own, &[]);
    let mut covers = ReleaseCovers { release: Vec::new(), album: Vec::new() };
    let copies = super::downscaled_copies::copies_json_sql(
        "source_release_cover_copy",
        "copy.catalog = cover.catalog AND copy.release_id = cover.release_id \
         AND copy.scope = cover.scope AND copy.position = cover.position",
    );
    for (scope, url, label, source, standing, copies_json) in sql.query(
        &format!(
            "SELECT cover.scope, cover.url, cover.label, cover.source, cover.standing, {copies} \
             FROM source_release_cover AS cover \
             WHERE cover.catalog = ? AND cover.release_id = ? \
             ORDER BY cover.scope, cover.position"
        ),
        params![catalog, key],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
    )? {
        let cover = RemoteCover {
            image: RemoteImageSet::with_copies(
                url,
                super::downscaled_copies::copies_of(&copies_json)?,
            ),
            label: super::cover_labels::label_of(&label)?,
            source: catalog_column(&source)?,
            standing: CoverStanding::from_column(&standing)
                .ok_or_else(|| DbError::unreadable("source_release_cover.standing", &standing))?,
        };
        match scope.as_str() {
            "release" => covers.release.push(cover),
            "album" => covers.album.push(cover),
            other => return Err(DbError::unreadable("source_release_cover.scope", other)),
        }
    }
    let archive_groups: Vec<String> = sql.query(
        "SELECT group_id FROM source_release_archive_group \
         WHERE catalog = ? AND release_id = ? ORDER BY position",
        params![catalog, key],
        |row| row.get(0),
    )?;
    let mediums = load_mediums(sql, catalog, key)?;
    pressing.facts.media = catalog_facts.stated_media(&mediums).counts();
    let unfetched = sql
        .query(
            "SELECT document, key, reason FROM source_release_unfetched \
             WHERE catalog = ? AND release_id = ? ORDER BY document, key",
            params![catalog, key],
            |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            },
        )?
        .into_iter()
        .map(|(document, document_key, reason)| {
            Ok(UnfetchedDocument {
                document: bae_catalogs::records::PayloadSource::from_str(&document).map_err(
                    |error| {
                        DbError::unreadable(
                            format!("source_release_unfetched.document ({error})"),
                            &document,
                        )
                    },
                )?,
                key: document_key,
                reason: match reason.as_str() {
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
    Ok(Some(SourceRelease {
        release: release.clone(),
        source_group_id,
        metadata: ReleaseMetadata {
            album: AlbumMetadata {
                title: album_title,
                artists: album_artists,
                year: derived_album_year(album_first_year, pressing.year),
                first_year: album_first_year,
            },
            pressing,
        },
        other_records,
        covers,
        archive_release,
        archive_groups,
        mediums,
        catalog: catalog_facts,
        notes,
        unfetched,
    }))
}

/// What the other catalogs say each of the stored `releases`, or its album,
/// is, as their rows hold it, in their order, in one statement: empty for a
/// release whose rows name no other catalog, or that nothing stored.
fn own_records_of_on<S: coven::SqlRead>(
    sql: &S,
    releases: &[&bae_catalogs::records::LookupRef],
) -> Result<Vec<Vec<ReleaseRecord>>, DbError> {
    let rows = sql.query(
        "SELECT w.key, r.record_catalog, r.kind, r.key, r.album_key \
         FROM json_each(?1) AS w \
         JOIN source_release_record AS r \
             ON r.catalog = w.value ->> 0 AND r.release_id = w.value ->> 1",
        [releases_json(releases)],
        |row| {
            Ok((
                row.get::<_, usize>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    let mut own: Vec<Vec<ReleaseRecord>> = vec![Vec::new(); releases.len()];
    for (at, record_catalog, kind, record_key, album_key) in rows {
        let reference = MetadataRef::new(catalog_column(&record_catalog)?, record_key);
        own[at].push(match kind.as_str() {
            "pressing" => ReleaseRecord::new(&reference, album_key),
            "album" => ReleaseRecord::album(&reference),
            other => return Err(DbError::unreadable("source_release_record.kind", other)),
        });
    }
    Ok(own)
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

/// `release` as import surfaces read it: with every album a kept statement
/// joins to one of the release's own — one its documents never reach.
///
/// The joined album is derived on every read, from the statement and the
/// release together: its record, and for a MusicBrainz release group the
/// archive's images of it. Nothing about it is stored with the release, so
/// the release reads the same whether it was fetched before the statement was
/// read or after, held in memory or stored, and stops naming the album when a
/// later reading stops stating it. The album's own text — its title, credits
/// and year — lives in documents a statement does not carry, so a joined
/// album contributes none; the release's own album documents state those.
fn with_joined_albums_on<S: coven::SqlRead>(
    sql: &S,
    stored: &SourceRelease,
) -> Result<SourceRelease, DbError> {
    let joined = super::album_link_rows::joined_albums_of_on(
        sql,
        &[super::album_link_rows::ReleaseAlbums {
            catalog: stored.release.catalog.catalog(),
            source_group_id: stored.source_group_id.as_deref(),
            records: &stored.other_records,
        }],
    )?
    .pop()
    .expect("one release has one entry");
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
    Ok(release)
}

/// The stored release `release` names, as import surfaces read it, or
/// `None` when no row names it.
pub(crate) fn load_source_release_on<S: coven::SqlRead>(
    sql: &S,
    release: &bae_catalogs::records::LookupRef,
) -> Result<Option<SourceRelease>, DbError> {
    load_source_release_rows_on(sql, release)?
        .map(|stored| with_joined_albums_on(sql, &stored))
        .transpose()
}

/// Store each of `releases` that a row names, in the write that wrote the
/// row: insert one not stored yet, and replace a stored one only where this
/// copy read a document the stored one lists as unfetched — a copy read
/// before the stored one was would otherwise overwrite a fuller read. A
/// release no row names is not stored: a catalog release is stored only while
/// something names it.
pub(crate) fn store_named_releases_on(
    sql: &CovenWrite<'_>,
    releases: &[SourceRelease],
) -> Result<(), DbError> {
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
        }
    }
    Ok(())
}

/// Every record each of the stored `releases` names, its own first — what
/// [`SourceRelease::records`] answers of it — read without the rest of the
/// release: its tracklist, covers and credits stay unread. `None` for one
/// nothing stored. Three statements whatever `releases` holds — their groups,
/// their records, and the albums kept for theirs — and none for no release.
pub(crate) fn load_releases_records_on<S: coven::SqlRead>(
    sql: &S,
    releases: &[&bae_catalogs::records::LookupRef],
) -> Result<Vec<Option<Vec<ReleaseRecord>>>, DbError> {
    if releases.is_empty() {
        return Ok(Vec::new());
    }
    let mut groups: Vec<Option<Option<String>>> = vec![None; releases.len()];
    for (at, source_group_id) in sql.query(
        "SELECT w.key, s.source_group_id FROM json_each(?1) AS w \
         JOIN source_release AS s ON s.catalog = w.value ->> 0 AND s.release_id = w.value ->> 1",
        [releases_json(releases)],
        |row| Ok((row.get::<_, usize>(0)?, row.get::<_, Option<String>>(1)?)),
    )? {
        groups[at] = Some(source_group_id);
    }
    let own = own_records_of_on(sql, releases)?;
    let stored: Vec<(usize, &Option<String>, &Vec<ReleaseRecord>)> = groups
        .iter()
        .zip(&own)
        .enumerate()
        .filter_map(|(at, (group, own))| group.as_ref().map(|group| (at, group, own)))
        .collect();
    let joined = super::album_link_rows::joined_albums_of_on(
        sql,
        &stored
            .iter()
            .map(|(at, group, own)| super::album_link_rows::ReleaseAlbums {
                catalog: releases[*at].catalog.catalog(),
                source_group_id: group.as_deref(),
                records: own,
            })
            .collect::<Vec<_>>(),
    )?;
    let mut records: Vec<Option<Vec<ReleaseRecord>>> = vec![None; releases.len()];
    for ((at, group, own), joined) in stored.into_iter().zip(joined) {
        records[at] = Some(bae_catalogs::source_release::release_records(
            releases[at],
            group.clone(),
            with_joined_records(own.clone(), &joined),
        ));
    }
    Ok(records)
}

impl ImportTables {
    /// The stored release `release` names, as import surfaces read it, or
    /// `None` when no row names it.
    pub async fn load_source_release(
        &self,
        release: &bae_catalogs::records::LookupRef,
    ) -> Result<Option<SourceRelease>, DbError> {
        let release = release.clone();
        self.store.read(move |sql| load_source_release_on(&sql, &release)).await
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
                load_source_release_rows_on(&sql, &release)?
                    .map(|stored| {
                        Ok(PreparedRelease {
                            release: with_joined_albums_on(&sql, &stored)?,
                            stored,
                        })
                    })
                    .transpose()
            })
            .await
    }

    /// `stored`, a release just fetched, beside how import surfaces read it.
    pub(crate) async fn prepared_release(
        &self,
        stored: SourceRelease,
    ) -> Result<PreparedRelease, DbError> {
        self.store
            .read(move |sql| {
                Ok(PreparedRelease { release: with_joined_albums_on(&sql, &stored)?, stored })
            })
            .await
    }

    /// Store `release` with no row naming it, which only a test's setup
    /// does: every other write stores a release in the write that names it.
    #[cfg(test)]
    pub async fn save_source_release(&self, release: &SourceRelease) -> Result<(), DbError> {
        let release = release.clone();
        self.store.call(move |sql| replace_source_release_on(sql, &release)).await
    }
}

#[cfg(test)]
#[path = "source_releases_tests.rs"]
mod tests;
