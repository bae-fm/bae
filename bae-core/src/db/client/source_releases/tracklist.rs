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
                  musicbrainz_artist_id, discogs_artist_id, join_phrase) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                credit.join,
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
    Performed {
        entry: i64,
    },
    Part {
        parent: i64,
        direction: PartDirection,
    },
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

/// A stored artist as a tracklist's JSON lists hold it.
#[derive(serde::Deserialize)]
struct StoredArtist {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
}

impl StoredArtist {
    fn artist(self) -> crate::import::ArtistCredit {
        crate::import::ArtistCredit {
            name: self.name,
            sort_name: self.sort_name,
            musicbrainz_artist_id: self.musicbrainz_artist_id,
            discogs_artist_id: self.discogs_artist_id,
            credited_as: None,
        }
    }
}

/// One entry credit as the entry's `credits` column holds it.
#[derive(serde::Deserialize)]
struct StoredCredit {
    position: i32,
    credited_name: String,
    /// Absent where the catalog printed a name with no artist behind it.
    artist: Option<StoredArtist>,
    join_phrase: Option<String>,
}

/// One entry role as the entry's `roles` column holds it.
#[derive(serde::Deserialize)]
struct StoredRole {
    position: i32,
    artist: StoredArtist,
    role: Option<String>,
}

/// A stored artist's columns under `alias`, as a JSON object.
fn artist_json(alias: &str) -> String {
    format!(
        "json_object('name', {alias}.name, 'sort_name', {alias}.sort_name, \
                     'musicbrainz_artist_id', {alias}.musicbrainz_artist_id, \
                     'discogs_artist_id', {alias}.discogs_artist_id)"
    )
}

fn stored_list<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json).map_err(|error| unreadable(what, format!("{json:?}: {error}")))
}

pub(super) fn load_mediums<S: QueryOne + QueryRows>(
    sql: &S,
    catalog: &str,
    key: &str,
) -> Result<Vec<SourceMedium>, DbError> {
    let mut mediums: Vec<SourceMedium> = sql
        .query(
            "SELECT medium.medium, \
                    (SELECT json_group_array(disc.disc_id ORDER BY disc.disc) \
                     FROM source_release_medium_disc AS disc \
                     WHERE disc.catalog = medium.catalog AND disc.release_id = medium.release_id \
                       AND disc.position = medium.position) AS disc_ids \
             FROM source_release_medium AS medium \
             WHERE medium.catalog = ? AND medium.release_id = ? ORDER BY medium.position",
            params![catalog, key],
            |row| {
                Ok((
                    super::pressing_columns::keyed(row, "medium", Medium::from_key)?,
                    row.get::<_, String>("disc_ids")?,
                ))
            },
        )?
        .into_iter()
        .map(|(medium, disc_ids)| {
            Ok(SourceMedium {
                medium,
                disc_ids: stored_list(&disc_ids, "medium Disc IDs")?,
                entries: Vec::new(),
            })
        })
        .collect::<Result<_, DbError>>()?;
    let rows = sql.query(
        &format!(
            "SELECT entry.entry, entry.medium, entry.parent, entry.kind, entry.position, \
                    entry.number, entry.title, entry.duration_ms, \
                    (SELECT json_group_array(json_object( \
                         'position', credit.position, 'credited_name', credit.credited_name, \
                         'artist', CASE WHEN credit.name IS NOT NULL \
                                       THEN json({credited}) END, \
                         'join_phrase', credit.join_phrase) ORDER BY credit.position) \
                     FROM source_release_entry_credit AS credit \
                     WHERE credit.catalog = entry.catalog AND credit.release_id = entry.release_id \
                       AND credit.entry = entry.entry) AS credits, \
                    (SELECT json_group_array(json_object( \
                         'position', role.position, 'artist', json({role_artist}), \
                         'role', role.role) ORDER BY role.position) \
                     FROM source_release_entry_role AS role \
                     WHERE role.catalog = entry.catalog AND role.release_id = entry.release_id \
                       AND role.entry = entry.entry) AS roles \
             FROM source_release_entry AS entry \
             WHERE entry.catalog = ? AND entry.release_id = ? ORDER BY entry.entry",
            credited = artist_json("credit"),
            role_artist = artist_json("role"),
        ),
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
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        },
    )?;
    let mut entries: Vec<EntryRow> = Vec::with_capacity(rows.len());
    let mut index_of: HashMap<i64, usize> = HashMap::with_capacity(rows.len());
    for (entry, medium, parent, kind, position, number, title, duration_ms, credits, roles) in rows
    {
        index_of.insert(entry, entries.len());
        let credits = stored_list::<StoredCredit>(&credits, "entry credits")?
            .into_iter()
            .map(|credit| DisplayCredit {
                position: credit.position,
                artist: credit.artist.map(|artist| {
                    let artist = artist.artist();
                    // The printed name is the credit's own where it is
                    // another than the artist's.
                    crate::import::ArtistCredit {
                        credited_as: Some(credit.credited_name.trim())
                            .filter(|printed| !printed.is_empty() && *printed != artist.name)
                            .map(str::to_string),
                        ..artist
                    }
                }),
                credited_name: credit.credited_name,
                join: credit.join_phrase,
            })
            .collect();
        let roles = stored_list::<StoredRole>(&roles, "entry roles")?
            .into_iter()
            .map(|role| RoleCredit {
                position: role.position,
                artist: role.artist.artist(),
                role: role.role,
            })
            .collect();
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
                credits,
                roles,
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

/// One stored work node, its events still the JSON its row holds.
struct WorkRow {
    node: i64,
    entry: Option<i64>,
    parent: Option<i64>,
    position: i32,
    direction: Option<PartDirection>,
    work: SourceWork,
    events: Vec<StoredEvent>,
}

/// One relation of a work as its node's `events` column holds it, in the
/// order of their positions: a composer, or the node of a part.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredEvent {
    Composer(StoredArtist),
    Part(i64),
}

/// Every work the release's tracks perform, each with its sub-graph, paired
/// with the entry that performs it.
fn load_works<S: QueryOne + QueryRows>(
    sql: &S,
    catalog: &str,
    key: &str,
) -> Result<Vec<(i64, PerformedWork)>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT work.node, work.entry, work.parent, work.position, work.direction, \
                    work.musicbrainz_work_id, work.title, work.disambiguation, work.work_type, \
                    (SELECT json_group_array(json(event.value) ORDER BY event.position) \
                     FROM (SELECT composer.position AS position, \
                                  json_object('composer', json({composer})) AS value \
                           FROM source_release_work_composer AS composer \
                           WHERE composer.catalog = work.catalog \
                             AND composer.release_id = work.release_id \
                             AND composer.node = work.node \
                           UNION ALL \
                           SELECT part.position, json_object('part', part.node) \
                           FROM source_release_work AS part \
                           WHERE part.catalog = work.catalog \
                             AND part.release_id = work.release_id \
                             AND part.parent = work.node) AS event) AS events \
             FROM source_release_work AS work \
             WHERE work.catalog = ? AND work.release_id = ? ORDER BY work.node",
            composer = artist_json("composer"),
        ),
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
                row.get::<_, String>(9)?,
            ))
        },
    )?;
    let mut nodes: Vec<WorkRow> = Vec::with_capacity(rows.len());
    for (node, entry, parent, position, direction, work, events) in rows {
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
            events: stored_list(&events, "work relations")?,
        });
    }
    let index_of: HashMap<i64, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, row)| (row.node, index))
        .collect();
    // Parts are numbered after the work they belong to, so completing nodes
    // from the last back finishes every part before its parent takes it.
    let mut parts: Vec<Option<(PartDirection, SourceWork)>> = vec![None; nodes.len()];
    let mut performed = Vec::new();
    for index in (0..nodes.len()).rev() {
        let row = &mut nodes[index];
        let mut work = row.work.clone();
        work.events = std::mem::take(&mut row.events)
            .into_iter()
            .map(|event| match event {
                StoredEvent::Composer(artist) => Ok(SourceWorkEvent::Composer(artist.artist())),
                StoredEvent::Part(node) => {
                    let part_index = *index_of
                        .get(&node)
                        .ok_or_else(|| unreadable("part of a missing work", node))?;
                    if part_index <= index {
                        return Err(unreadable("work parented under a later work", row.node));
                    }
                    let (direction, work) = parts[part_index]
                        .take()
                        .ok_or_else(|| unreadable("work that is a part twice", node))?;
                    Ok(SourceWorkEvent::Part { direction, work })
                }
            })
            .collect::<Result<_, DbError>>()?;
        match (row.entry, row.parent, row.direction) {
            (Some(entry), None, None) => performed.push((
                entry,
                PerformedWork {
                    position: row.position,
                    work,
                },
            )),
            (None, Some(_), Some(direction)) => parts[index] = Some((direction, work)),
            _ => {
                return Err(unreadable(
                    "work placed on neither a track nor a work",
                    row.node,
                ))
            }
        }
    }
    // A part whose work no node holds was never taken.
    if let Some(index) = parts.iter().position(Option::is_some) {
        return Err(unreadable("part of a missing work", nodes[index].node));
    }
    performed.reverse();
    Ok(performed)
}
