//! Each release editor's session on a library release, in device-local
//! tables: what the session opened on, the form with each change the person
//! made in it, the catalog release applied in it and the album picked in it.
//!
//! Each command an editor sends is one write to its session's rows, and the
//! write refuses a command to a session whose row is gone
//! ([`ReleaseEditSessionRefusal::Closed`]). The editor reads its draft from
//! the rows ([`ImportTables::subscribe_release_edit_draft`]). A save writes the
//! release and then the session again from the release as saved, in one
//! write; closing the editor deletes its session; no session outlives the
//! process, so bae empties the tables as the library opens. Deleting the
//! release deletes its sessions. This module's tables are in
//! `release_edit_session.sql`.

use super::import_list::{credit_resolutions_on, CreditSource};
use super::import_state::pane_rows::{
    assignment_kind_column, assignments_json_sql, assignments_of, barcodes_json_sql, decoded,
    insert_album_artist_assignments, insert_barcode_rows, insert_draft_values, insert_label_rows,
    insert_medium_rows, insert_track_artist_assignments, insert_track_values, labels_json_sql,
    mediums_json_sql, mediums_of, read_label_rows, release_roles_json_sql, roles_of,
    store_album_artist_assignments, track_roles_json_sql, track_works_json_sql,
    work_parts_json_sql, works_json_sql, works_of, DraftTables, StoredRole,
};
use super::*;
use crate::{
    AlbumPick, DraftAlbum, LibraryDraft, LibraryTrack, RawReleaseEdit, RawTrackEdit,
    ReleaseEditDraft, ReleaseEditLoadError, ReleaseEditSeed, TrackArtistAssignments,
};
use rusqlite::ToSql;
use seed::seed_on;
pub use seed::ReleaseEditContext;
#[cfg(any(test, feature = "test-utils"))]
use seed::{release_edit_rows_on, ReleaseEditRows};

mod seed;

/// The release edit sessions' part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("release_edit_session.sql")]);

/// The tables of a release editor's session's values.
pub(crate) const RELEASE_EDIT_SESSION_DRAFT: DraftTables = DraftTables {
    key: "session",
    label: "release_edit_label",
    barcode: "release_edit_barcode",
    album_artist: "release_edit_album_artist",
    track_artist: "release_edit_track_artist",
    medium: "release_edit_medium",
    track_role: "release_edit_track_role",
    release_role: "release_edit_release_role",
    work: "release_edit_work",
    work_credit: "release_edit_work_credit",
    work_part: "release_edit_work_part",
    track_work: "release_edit_track_work",
};

/// Why bae refused a write to a release editor's session. bae wrote none of
/// it.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseEditSessionRefusal {
    /// The session's rows are gone: its editor closed it, or someone deleted
    /// its release.
    #[error("release edit session {0} is closed")]
    Closed(i64),
    /// The library holds no release of this id.
    #[error("release {0} is not in the library")]
    ReleaseMissing(String),
    /// The form holds no track row of this id.
    #[error("the form holds no track row {0}")]
    RowMissing(String),
    /// The library holds no album of this id.
    #[error("album {0} is not in the library")]
    AlbumMissing(String),
    /// The library holds the release in a shape the editor can't read.
    #[error(transparent)]
    Unreadable(#[from] ReleaseEditLoadError),
}

/// Why a write to a release editor's session did not commit.
pub type ReleaseEditSessionError = WriteError<ReleaseEditSessionRefusal>;

/// What applying a catalog release in a session read: every value, laid
/// onto the release's tracks in order, and the records the pick names, the
/// applied one first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedCatalog {
    pub draft: LibraryDraft,
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
}

/// A release editor's session as its rows hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReleaseEdit {
    pub release_id: String,
    /// The release as the library held it when the session opened, or when
    /// its last save or cancel wrote it again.
    pub opened: ReleaseEditSeed,
    /// The form with each change the person made.
    pub form: RawReleaseEdit,
    pub applied: Option<AppliedCatalog>,
    pub album_pick: AlbumPick,
}

// ── Writing a session ───────────────────────────────────────────────────

/// The session row's form columns, in [`FORM_COLUMNS`] order.
const FORM_COLUMNS: &str =
    "album_title, album_year, year, country, region, media, status, packaging, discogs_details";

fn form_row_values(form: &RawReleaseEdit) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value;
    let facts = bae_records::records::pressing_columns::FactColumns::of(&form.pressing.facts);
    let optional = |value: Option<&str>| value.map_or(Value::Null, |v| Value::Text(v.to_string()));
    vec![
        Value::Text(form.album_title.clone()),
        Value::Text(form.album_year.clone()),
        Value::Text(form.pressing.year.clone()),
        optional(facts.country),
        optional(facts.region),
        Value::Text(bae_records::records::pressing_columns::media_column(
            &form.pressing.facts.media,
        )),
        optional(facts.status),
        optional(facts.packaging),
        Value::Text(facts.discogs_details),
    ]
}

/// `opened` as its column stores it.
fn opened_json(opened: &ReleaseEditSeed) -> Result<String, DbError> {
    serde_json::to_string(opened)
        .map_err(|error| DbError::encode("what a release edit session opened on", error))
}

/// Insert a session on `seed`'s release, with the form `seed` holds and
/// nothing applied or picked. Returns its id.
fn insert_session(
    sql: &CovenWrite<'_>,
    release_id: &str,
    seed: &ReleaseEditSeed,
) -> Result<i64, DbError> {
    let mut values = vec![
        rusqlite::types::Value::Text(release_id.to_string()),
        rusqlite::types::Value::Text(opened_json(seed)?),
    ];
    values.extend(form_row_values(&seed.edit));
    let session: i64 = sql.query_row(
        &format!(
            "INSERT INTO release_edit_session (release_id, opened, {FORM_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING session"
        ),
        rusqlite::params_from_iter(values),
        |row| row.get(0),
    )?;
    insert_form_rows(sql, session, &seed.edit)?;
    Ok(session)
}

/// Insert the form's rows below the session row, where the session holds
/// none.
fn insert_form_rows(
    sql: &CovenWrite<'_>,
    session: i64,
    form: &RawReleaseEdit,
) -> Result<(), DbError> {
    let tables = &RELEASE_EDIT_SESSION_DRAFT;
    let key: &dyn ToSql = &session;
    insert_label_rows(sql, tables, key, &form.pressing.labels)?;
    insert_barcode_rows(sql, tables, key, &form.pressing.barcodes)?;
    insert_album_artist_assignments(sql, tables, key, &form.album_artist_assignments)?;
    for (position, track) in form.tracks.iter().enumerate() {
        insert_track(sql, session, position, track)?;
    }
    Ok(())
}

/// One track row of the form and its own artists.
fn insert_track(
    sql: &CovenWrite<'_>,
    session: i64,
    position: usize,
    track: &RawTrackEdit,
) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO release_edit_track \
             (session, track_id, position, title, artist_assignment_kind) \
         VALUES (?, ?, ?, ?, ?)",
        params![
            session,
            track.id,
            position as i64,
            track.title,
            assignment_kind_column(&track.artist_assignments)
        ],
    )?;
    insert_track_artists(sql, session, track)
}

fn insert_track_artists(
    sql: &CovenWrite<'_>,
    session: i64,
    track: &RawTrackEdit,
) -> Result<(), DbError> {
    if let TrackArtistAssignments::Explicit(assignments) = &track.artist_assignments {
        insert_track_artist_assignments(
            sql,
            &RELEASE_EDIT_SESSION_DRAFT,
            &session,
            &track.id,
            assignments,
        )?;
    }
    Ok(())
}

/// Write `form` over `stored`, the form the session holds: only the rows
/// that differ. The two forms hold the same tracks, since no edit adds,
/// removes or moves one.
fn store_form(
    sql: &CovenWrite<'_>,
    session: i64,
    stored: &RawReleaseEdit,
    form: &RawReleaseEdit,
) -> Result<(), DbError> {
    let tables = &RELEASE_EDIT_SESSION_DRAFT;
    let key: &dyn ToSql = &session;
    let row = form_row_values(form);
    if form_row_values(stored) != row {
        let mut values = row;
        values.push(rusqlite::types::Value::Integer(session));
        sql.execute(
            &format!(
                "UPDATE release_edit_session SET ({FORM_COLUMNS}) = (?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 WHERE session = ?"
            ),
            rusqlite::params_from_iter(values),
        )?;
    }
    if stored.pressing.labels != form.pressing.labels {
        sql.execute("DELETE FROM release_edit_label WHERE session = ?", [session])?;
        insert_label_rows(sql, tables, key, &form.pressing.labels)?;
    }
    if stored.pressing.barcodes != form.pressing.barcodes {
        sql.execute("DELETE FROM release_edit_barcode WHERE session = ?", [session])?;
        insert_barcode_rows(sql, tables, key, &form.pressing.barcodes)?;
    }
    store_album_artist_assignments(
        sql,
        tables,
        key,
        &stored.album_artist_assignments,
        &form.album_artist_assignments,
    )?;
    for (stored, track) in stored.tracks.iter().zip(&form.tracks) {
        if stored.title != track.title
            || assignment_kind_column(&stored.artist_assignments)
                != assignment_kind_column(&track.artist_assignments)
        {
            sql.execute(
                "UPDATE release_edit_track SET title = ?, artist_assignment_kind = ? \
                 WHERE session = ? AND track_id = ?",
                params![
                    track.title,
                    assignment_kind_column(&track.artist_assignments),
                    session,
                    track.id
                ],
            )?;
        }
        if stored.artist_assignments != track.artist_assignments {
            sql.execute(
                "DELETE FROM release_edit_track_artist WHERE session = ? AND track_id = ?",
                params![session, track.id],
            )?;
            insert_track_artists(sql, session, track)?;
        }
    }
    Ok(())
}

/// Write what a catalog applied in the session read, where the session holds
/// no applied catalog: its mediums, works and roles, each track's place, and its
/// records.
fn insert_applied(
    sql: &CovenWrite<'_>,
    session: i64,
    applied: &AppliedCatalog,
) -> Result<(), DbError> {
    let tables = &RELEASE_EDIT_SESSION_DRAFT;
    let key: &dyn ToSql = &session;
    let AppliedCatalog { draft, records } = applied;
    sql.execute("INSERT INTO release_edit_applied (session) VALUES (?)", [session])?;
    insert_medium_rows(sql, tables, key, &draft.mediums)?;
    // Insert the works first, because a track names the works it performs.
    insert_draft_values(sql, tables, key, &draft.works, &draft.release_roles)?;
    for track in &draft.tracks {
        let stated_length_ms = track
            .stated_length_ms
            .map(|length| to_i64(length, "a track's stated length"))
            .transpose()?;
        sql.execute(
            "INSERT INTO release_edit_track_place \
                 (session, track_id, medium, side, track_number, stated_length_ms) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                session,
                track.edit.id,
                track.place.medium,
                track.place.side,
                track.place.number,
                stated_length_ms,
            ],
        )?;
        insert_track_values(sql, tables, key, &track.edit.id, &track.roles, &track.works)?;
    }
    for (position, record) in records.iter().enumerate() {
        use bae_catalogs::records::ReleaseRecord;
        let (catalog, kind, record_key, album_key) = match record {
            ReleaseRecord::Pressing { release, album_key } => {
                (release.catalog, "pressing", release.key.as_str(), album_key.as_deref())
            }
            ReleaseRecord::Album { album } => (album.catalog, "album", album.key.as_str(), None),
        };
        sql.execute(
            "INSERT INTO release_edit_record (session, position, catalog, kind, key, album_key) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![session, position as i64, catalog.as_str(), kind, record_key, album_key],
        )?;
    }
    Ok(())
}

/// Write the session again on `seed`: what it opened on, the form `seed`
/// holds, and nothing applied or picked.
fn reseed_on(sql: &CovenWrite<'_>, session: i64, seed: &ReleaseEditSeed) -> Result<(), DbError> {
    let mut values = vec![rusqlite::types::Value::Text(opened_json(seed)?)];
    values.extend(form_row_values(&seed.edit));
    values.push(rusqlite::types::Value::Integer(session));
    sql.execute(
        &format!(
            "UPDATE release_edit_session SET (opened, {FORM_COLUMNS}) = \
                 (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             WHERE session = ?"
        ),
        rusqlite::params_from_iter(values),
    )?;
    // Deleting a session's tracks also deletes their places, roles and works.
    for table in [
        "release_edit_applied",
        "release_edit_album_pick",
        "release_edit_label",
        "release_edit_barcode",
        "release_edit_album_artist",
        "release_edit_track",
    ] {
        sql.execute(&format!("DELETE FROM {table} WHERE session = ?"), [session])?;
    }
    insert_form_rows(sql, session, &seed.edit)
}

/// Write the session `session` again from its release as `sql` holds it
/// now, in place of everything it holds. The write refuses the session when
/// it or its release is gone. A save's write calls it after it writes the release.
pub fn reseed_release_edit_session_on(
    sql: &CovenWrite<'_>,
    session: i64,
) -> Result<Result<(), ReleaseEditSessionRefusal>, DbError> {
    let Some(release_id) = session_release_on(sql, session)? else {
        return Ok(Err(ReleaseEditSessionRefusal::Closed(session)));
    };
    let seed = match seed_on(sql, &release_id)? {
        Ok(seed) => seed,
        Err(refusal) => return Ok(Err(refusal)),
    };
    reseed_on(sql, session, &seed)?;
    Ok(Ok(()))
}

/// The check a save of the session `session` makes in its own write: the
/// session still holds `saved`, what the save read, and the library
/// still holds its release as the session opened on it. The check refuses
/// the save as closed once the session or its release is gone, and as stale
/// once the release changed. Each editor sends its session's commands one after another, so
/// a session that changed while its save ran is a bug, which fails the save.
pub fn release_edit_still_as_opened_on(
    sql: &CovenWrite<'_>,
    session: i64,
    saved: &StoredReleaseEdit,
) -> Result<Result<(), bae_records::ReleaseEditRefusal>, DbError> {
    use bae_records::ReleaseEditRefusal as R;
    let Some(held) = load_release_edit_session_on(sql, session)? else {
        return Ok(Err(R::Closed));
    };
    if held != *saved {
        return Err(DbError::unreadable(
            format!("release edit session {session} as its save read it"),
            "a session that changed while its save ran",
        ));
    }
    match seed_on(sql, &held.release_id)? {
        Ok(now) => Ok(if now == held.opened { Ok(()) } else { Err(R::Stale) }),
        Err(ReleaseEditSessionRefusal::ReleaseMissing(_)) => Ok(Err(R::Closed)),
        Err(refusal) => Err(DbError::unreadable(
            format!("release {} as an editor reads it", held.release_id),
            refusal,
        )),
    }
}

/// The release the session `session` edits; `None` once it is closed.
fn session_release_on(sql: &impl coven::SqlRead, session: i64) -> Result<Option<String>, DbError> {
    Ok(sql
        .query_row(
            "SELECT release_id FROM release_edit_session WHERE session = ?",
            [session],
            |row| row.get(0),
        )
        .optional()?)
}

// ── Reading a session ───────────────────────────────────────────────────

/// The session `session` as its rows hold it; `None` once it is closed.
pub(super) fn load_release_edit_session_on(
    sql: &impl coven::SqlRead,
    session: i64,
) -> Result<Option<StoredReleaseEdit>, DbError> {
    let tables = &RELEASE_EDIT_SESSION_DRAFT;
    let owner = "s.session";
    let track = format!(
        "json_object( \
             'track_id', t.track_id, 'title', t.title, 'kind', t.artist_assignment_kind, \
             'assignments', json({assignments}), \
             'place', json((SELECT json_object('medium', p.medium, 'side', p.side, \
                                                'number', p.track_number, \
                                                'stated_length_ms', p.stated_length_ms) \
                             FROM release_edit_track_place p \
                             WHERE p.session = t.session AND p.track_id = t.track_id)), \
             'roles', json({roles}), 'works', json({works}))",
        assignments = assignments_json_sql(
            tables.track_artist,
            "assignment.session = t.session AND assignment.track_id = t.track_id",
        ),
        roles = track_roles_json_sql(tables, "t.session", "t.track_id"),
        works = track_works_json_sql(tables, "t.session", "t.track_id"),
    );
    let row = sql
        .query_row(
            &format!(
                "SELECT s.release_id, s.opened, {FORM_COLUMNS}, \
                        {labels} AS labels, {barcodes} AS barcodes, \
                        {album_artists} AS album_artists, \
                        (SELECT json_group_array({track} ORDER BY t.position) \
                         FROM release_edit_track t WHERE t.session = s.session) AS tracks, \
                        EXISTS (SELECT 1 FROM release_edit_applied a WHERE a.session = s.session) \
                            AS applied, \
                        {mediums} AS mediums, {release_roles} AS release_roles, \
                        {works} AS works, {parts} AS parts, \
                        (SELECT json_group_array(json_object( \
                             'kind', r.kind, 'catalog', r.catalog, 'key', r.key, \
                             'album_key', r.album_key) ORDER BY r.position) \
                         FROM release_edit_record r WHERE r.session = s.session) AS records, \
                        pick.session IS NOT NULL AS picked, pick.album_id AS picked_album \
                 FROM release_edit_session s \
                 LEFT JOIN release_edit_album_pick pick ON pick.session = s.session \
                 WHERE s.session = ?",
                labels = labels_json_sql(tables, owner),
                barcodes = barcodes_json_sql(tables, owner),
                album_artists =
                    assignments_json_sql(tables.album_artist, "assignment.session = s.session",),
                mediums = mediums_json_sql(tables, owner),
                release_roles = release_roles_json_sql(tables, owner),
                works = works_json_sql(tables, owner),
                parts = work_parts_json_sql(tables, owner),
            ),
            [session],
            |row| {
                Ok(SessionRow {
                    release_id: row.get("release_id")?,
                    opened: row.get("opened")?,
                    album_title: row.get("album_title")?,
                    album_year: row.get("album_year")?,
                    year: row.get("year")?,
                    labels: read_label_rows(row)?,
                    barcodes: bae_records::records::pressing_columns::read_barcodes(
                        row, "barcodes",
                    )?,
                    facts: bae_records::records::pressing_columns::read_facts(row, "")?,
                    album_artists: row.get("album_artists")?,
                    tracks: row.get("tracks")?,
                    applied: row.get("applied")?,
                    mediums: row.get("mediums")?,
                    release_roles: row.get("release_roles")?,
                    works: row.get("works")?,
                    parts: row.get("parts")?,
                    records: bae_records::records::read::records_column(row, "records")?,
                    picked: row.get("picked")?,
                    picked_album: row.get("picked_album")?,
                })
            },
        )
        .optional()?;
    row.map(SessionRow::session).transpose()
}

/// One session's row as SQLite hands it over, its lists still JSON.
struct SessionRow {
    release_id: String,
    opened: String,
    album_title: String,
    album_year: String,
    year: String,
    labels: Vec<crate::RawLabelEdit>,
    barcodes: Vec<String>,
    facts: bae_types::pressing::PressingFacts,
    album_artists: String,
    tracks: String,
    applied: bool,
    mediums: String,
    release_roles: String,
    works: String,
    parts: String,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    picked: bool,
    picked_album: Option<String>,
}

/// One form track as the session's read holds it, with its applied place
/// and values where the person applied a catalog.
#[derive(serde::Deserialize)]
struct StoredSessionTrack {
    track_id: String,
    title: String,
    kind: String,
    assignments: serde_json::Value,
    place: Option<StoredPlace>,
    roles: Vec<StoredRole>,
    works: Vec<String>,
}

#[derive(serde::Deserialize)]
struct StoredPlace {
    medium: std::num::NonZeroU16,
    side: std::num::NonZeroU8,
    number: std::num::NonZeroU32,
    stated_length_ms: Option<i64>,
}

impl SessionRow {
    fn session(self) -> Result<StoredReleaseEdit, DbError> {
        let opened: ReleaseEditSeed = decoded(&self.opened, "opened release")?;
        let tracks: Vec<StoredSessionTrack> = decoded(&self.tracks, "tracks")?;
        let form_track = |track: &StoredSessionTrack| -> Result<RawTrackEdit, DbError> {
            let assignments = || assignments_of(&track.assignments.to_string(), "track artists");
            Ok(RawTrackEdit {
                id: track.track_id.clone(),
                title: track.title.clone(),
                artist_assignments: match track.kind.as_str() {
                    "album_artists" => TrackArtistAssignments::AlbumArtists,
                    "explicit" => TrackArtistAssignments::Explicit(assignments()?),
                    other => {
                        return Err(DbError::unreadable("a session track's artist kind", other))
                    }
                },
                file: None,
            })
        };
        let form = RawReleaseEdit {
            album_title: self.album_title,
            album_artist_assignments: assignments_of(&self.album_artists, "album artists")?,
            album_year: self.album_year,
            pressing: crate::RawPressingEdit {
                year: self.year,
                labels: self.labels,
                barcodes: self.barcodes,
                facts: self.facts,
            },
            tracks: tracks.iter().map(form_track).collect::<Result<_, _>>()?,
        };
        let applied = if self.applied {
            let draft_tracks = tracks
                .into_iter()
                .zip(&form.tracks)
                .map(|(track, edit)| {
                    let place = track.place.ok_or_else(|| {
                        DbError::unreadable("the applied place of a session track", &track.track_id)
                    })?;
                    Ok(crate::DraftTrack {
                        edit: RawTrackEdit {
                            id: edit.id.clone(),
                            title: edit.title.clone(),
                            artist_assignments: edit.artist_assignments.clone(),
                            file: LibraryTrack(edit.id.clone()),
                        },
                        place: bae_types::mediums::TrackPlace {
                            medium: place.medium,
                            side: place.side,
                            number: place.number,
                        },
                        stated_length_ms: place
                            .stated_length_ms
                            .map(|length| {
                                to_u64(length, "release_edit_track_place.stated_length_ms")
                            })
                            .transpose()?,
                        roles: roles_of(track.roles)?,
                        works: track.works,
                    })
                })
                .collect::<Result<_, DbError>>()?;
            Some(AppliedCatalog {
                draft: LibraryDraft {
                    album_title: form.album_title.clone(),
                    album_artist_assignments: form.album_artist_assignments.clone(),
                    album_year: form.album_year.clone(),
                    pressing: form.pressing.clone(),
                    mediums: mediums_of(&self.mediums)?,
                    tracks: draft_tracks,
                    release_roles: roles_of(decoded(&self.release_roles, "release roles")?)?,
                    works: works_of(
                        decoded(&self.works, "works")?,
                        decoded(&self.parts, "work parts")?,
                    )?,
                },
                records: self.records,
            })
        } else {
            None
        };
        let album_pick = match (self.picked, self.picked_album) {
            (false, _) => AlbumPick::Records,
            (true, None) => AlbumPick::New,
            (true, Some(album_id)) => AlbumPick::Album { album_id },
        };
        Ok(StoredReleaseEdit { release_id: self.release_id, opened, form, applied, album_pick })
    }
}

/// The album a save of the session `session` files its release under, read
/// in one statement: the one picked, else, with a catalog applied, the one
/// its records name (a new one where they name none the library holds), else
/// the album the release is in.
fn release_edit_album_on(sql: &impl coven::SqlRead, session: i64) -> Result<DraftAlbum, DbError> {
    let target = bae_records::records::release_records::records_target_album_sql(
        "SELECT position, catalog, kind, key, album_key FROM release_edit_record \
         WHERE session = :session",
        "(SELECT release_id FROM release_edit_session WHERE session = :session)",
    );
    let (kind, album_id, title, credits): (String, Option<String>, Option<String>, String) = sql
        .query_row(
            &format!(
                "WITH choice (kind, album_id) AS ( \
                     SELECT CASE WHEN pick.album_id IS NULL THEN 'new' ELSE 'album' END, \
                            pick.album_id \
                     FROM release_edit_album_pick pick WHERE pick.session = :session \
                     UNION ALL \
                     SELECT CASE WHEN target.target_album_id IS NULL THEN 'none' ELSE 'named' END, \
                            target.target_album_id \
                     FROM ({target}) AS target \
                     WHERE NOT EXISTS (SELECT 1 FROM release_edit_album_pick \
                                       WHERE session = :session) \
                       AND EXISTS (SELECT 1 FROM release_edit_applied WHERE session = :session) \
                     UNION ALL \
                     SELECT 'named', s.album_id \
                     FROM (SELECT r.album_id FROM release_edit_session s \
                           JOIN releases r ON r.id = s.release_id \
                           WHERE s.session = :session) AS s \
                     WHERE NOT EXISTS (SELECT 1 FROM release_edit_album_pick \
                                       WHERE session = :session) \
                       AND NOT EXISTS (SELECT 1 FROM release_edit_applied \
                                       WHERE session = :session)) \
                 SELECT choice.kind, a.id, a.title, {credits} \
                 FROM choice LEFT JOIN albums AS a ON a.id = choice.album_id",
                credits = bae_records::records::query::album_credits_json_sql(),
            ),
            named_params! { ":session": session },
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let album = |kind: &str| -> Result<(String, String, bae_types::credits::CreditLine), DbError> {
        match (album_id.clone(), title.clone()) {
            (Some(album_id), Some(title)) => {
                Ok((album_id, title, bae_records::records::read::credit_line_of_json(&credits)?))
            }
            _ => Err(DbError::unreadable(
                format!("the {kind} album of release edit session {session}"),
                None::<String>,
            )),
        }
    };
    Ok(match kind.as_str() {
        "none" => DraftAlbum::New,
        "new" => DraftAlbum::PickedNew,
        "named" => {
            let (album_id, title, artist_line) = album("named")?;
            DraftAlbum::Named { album_id, title, artist_line }
        }
        "album" => {
            let (album_id, title, artist_line) = album("picked")?;
            DraftAlbum::Picked { album_id, title, artist_line }
        }
        other => return Err(DbError::unreadable("the kind of a session's album", other)),
    })
}

/// The version of the cover of the session's release, `None` for none.
fn cover_version_on(sql: &impl coven::SqlRead, session: i64) -> Result<Option<String>, DbError> {
    Ok(sql.query_row(
        &format!(
            "SELECT {} FROM release_edit_session s WHERE s.session = ?",
            bae_records::records::files::cover_version_sql("s.release_id"),
        ),
        [session],
        |row| row.get(0),
    )?)
}

/// What a session's draft is read from: its rows, what the library holds
/// for each credit its form carries, the album a save files it under, its
/// release's cover, its release's tracks with their audio, placed on the
/// mediums a catalog applied in the session gives them or else on the
/// release's own, and the release's files.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseEditDraftRows {
    session: StoredReleaseEdit,
    artist_resolutions: Vec<bae_records::ResolvedCredit>,
    album: DraftAlbum,
    cover_version: Option<String>,
    tracks: Vec<bae_records::DbReleaseEditTrack>,
    files: bae_records::DbReleaseFiles,
}

/// The session's release, as an SQL expression over the parameter
/// `:session`.
const SESSION_RELEASE: &str =
    "(SELECT release_id FROM release_edit_session WHERE session = :session)";

/// The rows of the session `session`'s draft, one statement per part, each
/// binding only the session's id; `None` once it is closed.
pub(crate) fn release_edit_draft_rows_on(
    sql: &impl coven::SqlRead,
    session: i64,
) -> Result<Option<ReleaseEditDraftRows>, DbError> {
    let Some(stored) = load_release_edit_session_on(sql, session)? else {
        return Ok(None);
    };
    let not_applied = "NOT EXISTS (SELECT 1 FROM release_edit_applied WHERE session = :session)";
    let mediums = format!(
        "(SELECT position AS id, {SESSION_RELEASE} AS release_id, position, medium, side_count \
          FROM release_edit_medium WHERE session = :session \
          UNION ALL \
          SELECT id, release_id, position, medium, side_count FROM release_mediums \
          WHERE release_id = {SESSION_RELEASE} AND {not_applied})"
    );
    let places = format!(
        "(SELECT track_id AS id, medium AS medium_id, side, track_number \
          FROM release_edit_track_place WHERE session = :session \
          UNION ALL \
          SELECT id, medium_id, side, track_number FROM tracks \
          WHERE release_id = {SESSION_RELEASE} AND {not_applied})"
    );
    Ok(Some(ReleaseEditDraftRows {
        session: stored,
        artist_resolutions: credit_resolutions_on(sql, CreditSource::Session(&session))?,
        album: release_edit_album_on(sql, session)?,
        cover_version: cover_version_on(sql, session)?,
        tracks: bae_records::records::release_edit_tracks_on(
            sql,
            SESSION_RELEASE,
            bae_records::records::MediumsSource::Draft { mediums: &mediums, places: &places },
            named_params! { ":session": session },
        )?,
        files: bae_records::records::release_files_on(
            sql,
            SESSION_RELEASE,
            named_params! { ":session": session },
        )?,
    }))
}

impl ReleaseEditDraftRows {
    /// The draft the editor shows: the form, the tracks laid out, whether a
    /// save would write anything the session did not open on, and why the
    /// editor can't save the form, when it can't.
    pub(crate) fn draft(self) -> Result<ReleaseEditDraft, DbError> {
        let Self { session, artist_resolutions, album, cover_version, tracks, files } = self;
        let StoredReleaseEdit { release_id, opened, form, applied, album_pick } = session;
        let changed = form != opened.edit || applied.is_some() || album_pick != AlbumPick::Records;
        let display =
            crate::ReleaseEditDisplayContext::from_raw(&files, tracks).map_err(|error| {
                DbError::unreadable(format!("the tracks of release {release_id}"), error)
            })?;
        let invalid = form.shape().err();
        Ok(ReleaseEditDraft {
            edit: form,
            cover: bae_records::cover_ref(&release_id, cover_version),
            display,
            album,
            artist_resolutions,
            changed,
            invalid,
        })
    }
}

impl ImportTables {
    /// Open a session on the release `release_id` as the library holds it,
    /// with nothing edited, applied or picked. Returns its id.
    pub async fn open_release_edit_session(
        &self,
        release_id: &str,
    ) -> Result<i64, ReleaseEditSessionError> {
        let release_id = release_id.to_string();
        self.store
            .write(move |sql| {
                let seed = seed_on(&sql, &release_id)?.map_err(WriteError::Refused)?;
                Ok(insert_session(&sql, &release_id, &seed)?)
            })
            .await
    }

    /// Write one change of the form, as `change` makes it from the form the
    /// session holds. `change` refuses a change it can't make, writing
    /// nothing.
    pub async fn change_release_edit_session(
        &self,
        session: i64,
        change: impl FnOnce(&mut RawReleaseEdit) -> Result<(), ReleaseEditSessionRefusal>
            + Send
            + 'static,
    ) -> Result<(), ReleaseEditSessionError> {
        self.store
            .write(move |sql| {
                let stored = load_release_edit_session_on(&sql, session)?
                    .ok_or(WriteError::Refused(ReleaseEditSessionRefusal::Closed(session)))?
                    .form;
                let mut form = stored.clone();
                change(&mut form).map_err(WriteError::Refused)?;
                Ok(store_form(&sql, session, &stored, &form)?)
            })
            .await
    }

    /// Write `applied` into the session: its values become the form, in
    /// place of the catalog the person applied before.
    pub async fn apply_to_release_edit_session(
        &self,
        session: i64,
        applied: AppliedCatalog,
    ) -> Result<(), ReleaseEditSessionError> {
        self.store
            .write(move |sql| {
                let stored = load_release_edit_session_on(&sql, session)?
                    .ok_or(WriteError::Refused(ReleaseEditSessionRefusal::Closed(session)))?
                    .form;
                store_form(&sql, session, &stored, &applied.draft.release_edit())?;
                sql.execute("DELETE FROM release_edit_applied WHERE session = ?", [session])?;
                Ok(insert_applied(&sql, session, &applied)?)
            })
            .await
    }

    /// Write `pick` as the album the session files its release under. The
    /// write refuses an album the library does not hold.
    pub async fn pick_release_edit_album(
        &self,
        session: i64,
        pick: AlbumPick,
    ) -> Result<(), ReleaseEditSessionError> {
        self.store
            .write(move |sql| {
                if session_release_on(&sql, session)?.is_none() {
                    return Err(WriteError::Refused(ReleaseEditSessionRefusal::Closed(session)));
                }
                let album_id = match pick {
                    AlbumPick::Records => {
                        sql.execute(
                            "DELETE FROM release_edit_album_pick WHERE session = ?",
                            [session],
                        )?;
                        return Ok(());
                    }
                    AlbumPick::New => None,
                    AlbumPick::Album { album_id } => {
                        let held: bool = sql.query_row(
                            "SELECT EXISTS (SELECT 1 FROM albums WHERE id = ?)",
                            [&album_id],
                            |row| row.get(0),
                        )?;
                        if !held {
                            return Err(WriteError::Refused(
                                ReleaseEditSessionRefusal::AlbumMissing(album_id),
                            ));
                        }
                        Some(album_id)
                    }
                };
                sql.execute(
                    "INSERT INTO release_edit_album_pick (session, album_id) VALUES (?, ?) \
                     ON CONFLICT (session) DO UPDATE SET album_id = excluded.album_id",
                    params![session, album_id],
                )?;
                Ok(())
            })
            .await
    }

    /// Write the session again from its release as the library holds it
    /// now: what Cancel does.
    pub async fn reseed_release_edit_session(
        &self,
        session: i64,
    ) -> Result<(), ReleaseEditSessionError> {
        self.store
            .write(move |sql| {
                reseed_release_edit_session_on(&sql, session)?.map_err(WriteError::Refused)
            })
            .await
    }

    /// Delete the session `session` and every row it holds.
    pub async fn close_release_edit_session(&self, session: i64) -> Result<(), DbError> {
        self.store
            .call(move |sql| {
                sql.execute("DELETE FROM release_edit_session WHERE session = ?", [session])?;
                Ok(())
            })
            .await
    }

    /// Delete every session: what the library opens with, since no session
    /// outlives the process that opened it.
    pub async fn clear_release_edit_sessions(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.execute("DELETE FROM release_edit_session", [])?;
                Ok(())
            })
            .await
    }

    /// The session `session` as its rows hold it; `None` once it is closed.
    pub async fn load_release_edit_session(
        &self,
        session: i64,
    ) -> Result<Option<StoredReleaseEdit>, DbError> {
        self.store.read(move |sql| load_release_edit_session_on(&sql, session)).await
    }

    /// The session `session`'s draft, live: it reruns after each write that
    /// changes what it read, and reads `None` once the session is closed.
    pub fn subscribe_release_edit_draft(
        &self,
        session: i64,
    ) -> coven::LiveQuery<Option<ReleaseEditDraft>, DbError> {
        self.store
            .subscribe(move |sql| release_edit_draft_rows_on(&sql, session))
            .process(|rows| rows.map(ReleaseEditDraftRows::draft).transpose())
    }

    /// The release `release_id` as the editor reads it, from one read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_release_edit_context(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseEditContext>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| release_edit_rows_on(&sql, &release_id))
            .process(|rows| Ok(rows.map(ReleaseEditRows::processed)))
            .await
    }
}
