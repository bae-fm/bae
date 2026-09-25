//! The `source_release` tables: every catalog release bae fetched, as its
//! extraction left it.
//!
//! A release is written whole when it is fetched and read whole by every
//! import surface. Writing it again replaces every row under it in one
//! transaction, so a reader sees one extraction or the other, never a mix.

use super::*;
use crate::import::assemble::{ArtistRef, PartDirection};
use crate::import::cover_art::RemoteCover;
use crate::import::release_metadata::{AlbumMetadata, ReleaseMetadata};
use crate::import::source_release::{
    ArchiveRelease, ArtistCredit, CatalogFacts, EntryKind, PerformedWork, ReleaseCovers,
    RoleCredit, SourceMedium, SourceRelease, SourceWork, SourceWorkEvent, TracklistEntry,
    UnfetchedDocument, UnfetchedReason,
};
use crate::import::{MetadataRef, ReleaseRecord};

/// The tables under `source_release`, children before the tables they hang
/// off. Deleting a release's mediums takes its entries, and with them their
/// credits, roles and works.
const CHILD_TABLES: [&str; 9] = [
    "source_release_album_artist",
    "source_release_link",
    "source_release_format",
    "source_release_record",
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
    sql.execute(
        "INSERT INTO source_release \
             (catalog, release_id, source_group_id, album_title, album_year, year, format, \
              label, catalog_number, country, barcode, archive_release_id, archive_group_id, \
              fetched_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (catalog, release_id) DO UPDATE SET \
             source_group_id = excluded.source_group_id, \
             album_title = excluded.album_title, album_year = excluded.album_year, \
             year = excluded.year, format = excluded.format, label = excluded.label, \
             catalog_number = excluded.catalog_number, country = excluded.country, \
             barcode = excluded.barcode, archive_release_id = excluded.archive_release_id, \
             archive_group_id = excluded.archive_group_id, fetched_at = excluded.fetched_at",
        params![
            catalog,
            key,
            release.source_group_id,
            metadata.album.title,
            metadata.album.year,
            pressing.year,
            pressing.format,
            pressing.label,
            pressing.catalog_number,
            pressing.country,
            pressing.barcode,
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
            formats,
            release_roles,
        } => {
            for (position, descriptor) in formats.iter().enumerate() {
                sql.execute(
                    "INSERT INTO source_release_format (catalog, release_id, position, descriptor) \
                     VALUES (?, ?, ?, ?)",
                    params![catalog, key, position as i64, descriptor],
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
                     (catalog, release_id, scope, position, url, thumbnail_url, label, source) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    catalog,
                    key,
                    scope,
                    position as i64,
                    cover.url,
                    cover.thumbnail_url,
                    cover.label,
                    cover.source.as_str(),
                ],
            )?;
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
            "INSERT INTO source_release_medium (catalog, release_id, position, format) \
             VALUES (?, ?, ?, ?)",
            params![catalog, key, position as i64, medium.format],
        )?;
        for entry in &medium.entries {
            insert_entry(sql, catalog, key, position as i64, None, entry, &mut numbering)?;
        }
    }
    Ok(())
}

/// The next entry and work node numbers of the release being written.
#[derive(Default)]
struct Numbering {
    entry: i64,
    node: i64,
}

fn insert_entry(
    sql: &SqlContext<'_, '_>,
    catalog: &str,
    key: &str,
    medium: i64,
    parent: Option<i64>,
    entry: &TracklistEntry,
    numbering: &mut Numbering,
) -> Result<(), DbError> {
    let number = numbering.entry;
    numbering.entry += 1;
    sql.execute(
        "INSERT INTO source_release_entry \
             (catalog, release_id, entry, medium, parent, kind, position, number, title, \
              duration_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            catalog,
            key,
            number,
            medium,
            parent,
            entry.kind.as_str(),
            entry.position,
            entry.number,
            entry.title,
            entry
                .duration_ms
                .map(|duration| to_i64(duration, "track duration"))
                .transpose()?,
        ],
    )?;
    for credit in &entry.credits {
        let artist = credit.artist.as_ref();
        sql.execute(
            "INSERT INTO source_release_entry_credit \
                 (catalog, release_id, entry, position, credited_name, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                catalog,
                key,
                number,
                credit.position,
                credit.credited_name,
                artist.map(|artist| artist.name.as_str()),
                artist.and_then(|artist| artist.sort_name.as_deref()),
                artist.and_then(|artist| artist.musicbrainz_artist_id.as_deref()),
                artist.and_then(|artist| artist.discogs_artist_id.as_deref()),
            ],
        )?;
    }
    for role in &entry.roles {
        sql.execute(
            "INSERT INTO source_release_entry_role \
                 (catalog, release_id, entry, position, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id, role) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                catalog,
                key,
                number,
                role.position,
                role.artist.name,
                role.artist.sort_name,
                role.artist.musicbrainz_artist_id,
                role.artist.discogs_artist_id,
                role.role,
            ],
        )?;
    }
    for performed in &entry.works {
        insert_work(
            sql,
            catalog,
            key,
            WorkPlace::Performed { entry: number },
            performed.position,
            &performed.work,
            numbering,
        )?;
    }
    for child in &entry.children {
        insert_entry(sql, catalog, key, medium, Some(number), child, numbering)?;
    }
    Ok(())
}

/// Where a stored work node hangs: off the track that performs it, or off
/// the work it is a part relation of.
enum WorkPlace {
    Performed { entry: i64 },
    Part { parent: i64, direction: PartDirection },
}

fn direction_column(direction: PartDirection) -> &'static str {
    match direction {
        PartDirection::Forward => "forward",
        PartDirection::Backward => "backward",
    }
}

fn insert_work(
    sql: &SqlContext<'_, '_>,
    catalog: &str,
    key: &str,
    place: WorkPlace,
    position: i32,
    work: &SourceWork,
    numbering: &mut Numbering,
) -> Result<(), DbError> {
    let node = numbering.node;
    numbering.node += 1;
    let (entry, parent, direction) = match place {
        WorkPlace::Performed { entry } => (Some(entry), None, None),
        WorkPlace::Part { parent, direction } => {
            (None, Some(parent), Some(direction_column(direction)))
        }
    };
    sql.execute(
        "INSERT INTO source_release_work \
             (catalog, release_id, node, entry, parent, position, direction, \
              musicbrainz_work_id, title, disambiguation, work_type) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            catalog,
            key,
            node,
            entry,
            parent,
            position,
            direction,
            work.musicbrainz_work_id,
            work.title,
            work.disambiguation,
            work.work_type,
        ],
    )?;
    for (event_position, event) in work.events.iter().enumerate() {
        match event {
            SourceWorkEvent::Composer(artist) => {
                sql.execute(
                    "INSERT INTO source_release_work_composer \
                         (catalog, release_id, node, position, name, sort_name, \
                          musicbrainz_artist_id, discogs_artist_id) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        catalog,
                        key,
                        node,
                        event_position as i64,
                        artist.name,
                        artist.sort_name,
                        artist.musicbrainz_artist_id,
                        artist.discogs_artist_id,
                    ],
                )?;
            }
            SourceWorkEvent::Part { direction, work } => {
                insert_work(
                    sql,
                    catalog,
                    key,
                    WorkPlace::Part {
                        parent: node,
                        direction: *direction,
                    },
                    event_position as i32,
                    work,
                    numbering,
                )?;
            }
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
pub(super) fn load_source_release_on<S: QueryOne + QueryRows>(
    sql: &S,
    release: &MetadataRef,
) -> Result<Option<SourceRelease>, DbError> {
    let catalog = release.catalog.as_str();
    let key = release.key.as_str();
    let head = sql
        .query_row(
            "SELECT source_group_id, album_title, album_year, year, format, label, \
                    catalog_number, country, barcode, archive_release_id, archive_group_id \
             FROM source_release WHERE catalog = ? AND release_id = ?",
            params![catalog, key],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i32>>(2)?,
                    Pressing {
                        year: row.get(3)?,
                        format: row.get(4)?,
                        label: row.get(5)?,
                        catalog_number: row.get(6)?,
                        country: row.get(7)?,
                        barcode: row.get(8)?,
                    },
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
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
            formats: sql.query(
                "SELECT descriptor FROM source_release_format \
                 WHERE catalog = ? AND release_id = ? ORDER BY position",
                params![catalog, key],
                |row| row.get(0),
            )?,
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
    other_records.sort_by_key(|record| crate::import::source_release::catalog_rank(record.catalog()));
    let mut covers = ReleaseCovers {
        release: Vec::new(),
        album: Vec::new(),
    };
    for (scope, url, thumbnail_url, label, source) in sql.query(
        "SELECT scope, url, thumbnail_url, label, source FROM source_release_cover \
         WHERE catalog = ? AND release_id = ? ORDER BY scope, position",
        params![catalog, key],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )? {
        let cover = RemoteCover {
            url,
            thumbnail_url,
            label,
            source: catalog_column(&source)?,
        };
        match scope.as_str() {
            "release" => covers.release.push(cover),
            "album" => covers.album.push(cover),
            other => return Err(unreadable("cover scope", other)),
        }
    }
    let archive_groups = sql.query(
        "SELECT group_id FROM source_release_archive_group \
         WHERE catalog = ? AND release_id = ? ORDER BY position",
        params![catalog, key],
        |row| row.get(0),
    )?;
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

/// One stored entry, before its children and works are attached.
struct EntryRow {
    medium: i64,
    parent: Option<i64>,
    value: TracklistEntry,
}

fn load_mediums<S: QueryOne + QueryRows>(
    sql: &S,
    catalog: &str,
    key: &str,
) -> Result<Vec<SourceMedium>, DbError> {
    let mut mediums: Vec<SourceMedium> = sql
        .query(
            "SELECT format FROM source_release_medium \
             WHERE catalog = ? AND release_id = ? ORDER BY position",
            params![catalog, key],
            |row| {
                Ok(SourceMedium {
                    format: row.get(0)?,
                    entries: Vec::new(),
                })
            },
        )?;
    let rows = sql.query(
        "SELECT entry, medium, parent, kind, position, number, title, duration_ms \
         FROM source_release_entry WHERE catalog = ? AND release_id = ? ORDER BY entry",
        params![catalog, key],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        },
    )?;
    let mut entries: Vec<EntryRow> = Vec::with_capacity(rows.len());
    let mut index_of: HashMap<i64, usize> = HashMap::with_capacity(rows.len());
    for (entry, medium, parent, kind, position, number, title, duration_ms) in rows {
        index_of.insert(entry, entries.len());
        entries.push(EntryRow {
            medium,
            parent,
            value: TracklistEntry {
                kind: EntryKind::parse(&kind).ok_or_else(|| unreadable("entry kind", &kind))?,
                position,
                number,
                title,
                duration_ms: duration_ms
                    .map(|duration| to_u64(duration, "track duration"))
                    .transpose()?,
                credits: Vec::new(),
                roles: Vec::new(),
                works: Vec::new(),
                children: Vec::new(),
            },
        });
    }
    let entry_at = |entry: i64| -> Result<usize, DbError> {
        index_of
            .get(&entry)
            .copied()
            .ok_or_else(|| unreadable("reference to a missing entry", entry))
    };
    for (entry, credit) in sql.query(
        "SELECT entry, position, credited_name, name, sort_name, musicbrainz_artist_id, \
                discogs_artist_id \
         FROM source_release_entry_credit WHERE catalog = ? AND release_id = ? \
         ORDER BY entry, position",
        params![catalog, key],
        |row| {
            let name: Option<String> = row.get(3)?;
            Ok((
                row.get::<_, i64>(0)?,
                ArtistCredit {
                    position: row.get(1)?,
                    credited_name: row.get(2)?,
                    artist: match name {
                        Some(name) => Some(ArtistRef {
                            name,
                            sort_name: row.get(4)?,
                            musicbrainz_artist_id: row.get(5)?,
                            discogs_artist_id: row.get(6)?,
                        }),
                        None => None,
                    },
                },
            ))
        },
    )? {
        let index = entry_at(entry)?;
        entries[index].value.credits.push(credit);
    }
    for (entry, role) in sql.query(
        "SELECT entry, position, name, sort_name, musicbrainz_artist_id, discogs_artist_id, role \
         FROM source_release_entry_role WHERE catalog = ? AND release_id = ? \
         ORDER BY entry, position",
        params![catalog, key],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                RoleCredit {
                    position: row.get(1)?,
                    artist: artist_at(row, 2)?,
                    role: row.get(6)?,
                },
            ))
        },
    )? {
        let index = entry_at(entry)?;
        entries[index].value.roles.push(role);
    }
    for (entry, performed) in load_works(sql, catalog, key)? {
        let index = entry_at(entry)?;
        entries[index].value.works.push(performed);
    }
    // Children follow their parent in entry order, so attaching from the
    // last row back completes every child before its parent takes it.
    let mut attached: Vec<Option<TracklistEntry>> = Vec::with_capacity(entries.len());
    let mut places = Vec::with_capacity(entries.len());
    for row in entries {
        places.push((row.medium, row.parent));
        attached.push(Some(row.value));
    }
    for index in (0..attached.len()).rev() {
        let (_, Some(parent)) = places[index] else {
            continue;
        };
        let parent_index = entry_at(parent)?;
        if parent_index >= index {
            return Err(unreadable("entry parented under a later entry", parent));
        }
        let child = attached[index].take().expect("each entry is attached once");
        attached[parent_index]
            .as_mut()
            .expect("a parent is attached after its children")
            .children
            .insert(0, child);
    }
    for (value, (medium, _)) in attached.into_iter().zip(places) {
        let Some(value) = value else {
            continue;
        };
        let medium = usize::try_from(medium)
            .ok()
            .and_then(|medium| mediums.get_mut(medium))
            .ok_or_else(|| unreadable("reference to a missing medium", medium))?;
        medium.entries.push(value);
    }
    Ok(mediums)
}

/// One stored work node, before its events are attached.
struct WorkRow {
    node: i64,
    entry: Option<i64>,
    parent: Option<i64>,
    position: i32,
    direction: Option<PartDirection>,
    work: SourceWork,
}

/// Every work the release's tracks perform, each with its sub-graph, paired
/// with the entry that performs it.
fn load_works<S: QueryOne + QueryRows>(
    sql: &S,
    catalog: &str,
    key: &str,
) -> Result<Vec<(i64, PerformedWork)>, DbError> {
    let rows = sql.query(
        "SELECT node, entry, parent, position, direction, musicbrainz_work_id, title, \
                disambiguation, work_type \
         FROM source_release_work WHERE catalog = ? AND release_id = ? ORDER BY node",
        params![catalog, key],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                SourceWork {
                    musicbrainz_work_id: row.get(5)?,
                    title: row.get(6)?,
                    disambiguation: row.get(7)?,
                    work_type: row.get(8)?,
                    events: Vec::new(),
                },
            ))
        },
    )?;
    let mut nodes: Vec<WorkRow> = Vec::with_capacity(rows.len());
    for (node, entry, parent, position, direction, work) in rows {
        let direction = match direction.as_deref() {
            None => None,
            Some("forward") => Some(PartDirection::Forward),
            Some("backward") => Some(PartDirection::Backward),
            Some(other) => return Err(unreadable("part direction", other)),
        };
        nodes.push(WorkRow {
            node,
            entry,
            parent,
            position,
            direction,
            work,
        });
    }
    let index_of: HashMap<i64, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, row)| (row.node, index))
        .collect();
    // A node's events, by their position among its relations.
    let mut events: Vec<Vec<(i32, SourceWorkEvent)>> = vec![Vec::new(); nodes.len()];
    for (node, position, composer) in sql.query(
        "SELECT node, position, name, sort_name, musicbrainz_artist_id, discogs_artist_id \
         FROM source_release_work_composer WHERE catalog = ? AND release_id = ?",
        params![catalog, key],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i32>(1)?, artist_at(row, 2)?)),
    )? {
        let index = *index_of
            .get(&node)
            .ok_or_else(|| unreadable("composer of a missing work", node))?;
        events[index].push((position, SourceWorkEvent::Composer(composer)));
    }
    // Parts are numbered after the work they belong to, so completing nodes
    // from the last back finishes every part before its parent takes it.
    let mut performed = Vec::new();
    for index in (0..nodes.len()).rev() {
        let row = &nodes[index];
        let mut node_events = std::mem::take(&mut events[index]);
        node_events.sort_by_key(|(position, _)| *position);
        let mut work = row.work.clone();
        work.events = node_events.into_iter().map(|(_, event)| event).collect();
        match (row.entry, row.parent, row.direction) {
            (Some(entry), None, None) => performed.push((
                entry,
                PerformedWork {
                    position: row.position,
                    work,
                },
            )),
            (None, Some(parent), Some(direction)) => {
                let parent_index = *index_of
                    .get(&parent)
                    .ok_or_else(|| unreadable("part of a missing work", parent))?;
                if parent_index >= index {
                    return Err(unreadable("work parented under a later work", parent));
                }
                events[parent_index].push((row.position, SourceWorkEvent::Part { direction, work }));
            }
            _ => return Err(unreadable("work placed on neither a track nor a work", row.node)),
        }
    }
    performed.reverse();
    Ok(performed)
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
