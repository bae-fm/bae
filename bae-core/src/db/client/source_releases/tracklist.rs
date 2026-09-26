//! A stored release's tracklist: its mediums' entries in order, with each
//! entry's credits, roles and the works it performs — written and read back
//! as one tree.

use super::*;

/// The next entry and work node numbers of the release being written.
#[derive(Default)]
pub(super) struct Numbering {
    entry: i64,
    node: i64,
}

pub(super) fn insert_entry(
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

/// One stored entry, before its children and works are attached.
struct EntryRow {
    medium: i64,
    parent: Option<i64>,
    value: TracklistEntry,
}

pub(super) fn load_mediums<S: QueryOne + QueryRows>(
    sql: &S,
    catalog: &str,
    key: &str,
) -> Result<Vec<SourceMedium>, DbError> {
    let mut mediums: Vec<SourceMedium> = sql
        .query(
            "SELECT medium FROM source_release_medium \
             WHERE catalog = ? AND release_id = ? ORDER BY position",
            params![catalog, key],
            |row| {
                Ok(SourceMedium {
                    medium: super::pressing_columns::keyed(row, "medium", Medium::from_key)?,
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
