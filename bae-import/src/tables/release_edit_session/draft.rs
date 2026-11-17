//! The draft a release editor shows, read from its session's rows and the
//! library: the form, the album a save files the release under, the
//! release's cover and files, and its tracks, each beside the form row that
//! edits it.

use super::*;

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
/// release's own, each with its form row, whether the release has tracks,
/// and the release's files.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseEditDraftRows {
    session: StoredReleaseEdit,
    artist_resolutions: Vec<bae_records::ResolvedCredit>,
    album: DraftAlbum,
    cover_version: Option<String>,
    tracks: Vec<DraftTrackRow>,
    has_tracks: bool,
    files: bae_records::DbReleaseFiles,
}

/// One track of a session's draft as its statement returns it: what the
/// library holds about the track, and the form row that edits it as JSON,
/// `None` when the form holds no row for the track.
#[derive(Debug, Clone, PartialEq)]
struct DraftTrackRow {
    track: bae_records::DbReleaseEditTrack,
    form: Option<String>,
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
        tracks: draft_tracks_on(
            sql,
            bae_records::records::MediumsSource::Draft { mediums: &mediums, places: &places },
            session,
        )?,
        has_tracks: sql.query_row(
            &format!("SELECT EXISTS (SELECT 1 FROM tracks WHERE release_id = {SESSION_RELEASE})"),
            named_params! { ":session": session },
            |row| row.get(0),
        )?,
        files: bae_records::records::release_files_on(
            sql,
            SESSION_RELEASE,
            named_params! { ":session": session },
        )?,
    }))
}

/// The tracks of the session `session`'s release in release order, placed
/// on the mediums `source` names, each with the row of the session's form
/// that edits it.
fn draft_tracks_on(
    sql: &impl coven::SqlRead,
    source: bae_records::records::MediumsSource<'_>,
    session: i64,
) -> Result<Vec<DraftTrackRow>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT library.*, \
                    (SELECT {form} FROM release_edit_track t \
                     WHERE t.session = :session AND t.track_id = library.id) AS form \
             FROM ({tracks}) AS library ORDER BY library.position",
            form = form_track_json_sql(),
            tracks = bae_records::records::release_edit_tracks_sql(SESSION_RELEASE, source),
        ),
        named_params! { ":session": session },
        |row| {
            Ok(DraftTrackRow {
                track: bae_records::records::release_edit_track_of_row(row)?,
                form: row.get("form")?,
            })
        },
    )?)
}

impl DraftTrackRow {
    /// The row of the editor's track table this track is. `unreadable` says
    /// what makes the track one the editor can't show.
    fn table_row(
        self,
        unreadable: &impl Fn(crate::ReleaseEditLoadError) -> DbError,
    ) -> Result<crate::ReleaseEditTrackRow, DbError> {
        let track_id = self.track.track.id.clone();
        let form = self
            .form
            .ok_or_else(|| unreadable(crate::ReleaseEditLoadError::MissingFormRow { track_id }))?;
        let stored: StoredSessionTrack = decoded(&form, "a track's form row")?;
        Ok(crate::ReleaseEditTrackRow {
            edit: stored.form_row()?,
            context: crate::ReleaseEditTrackContext::of_track(self.track).map_err(unreadable)?,
        })
    }
}

impl ReleaseEditDraftRows {
    /// The draft the editor shows: the form, the tracks laid out, whether a
    /// save would write anything the session did not open on, and why the
    /// editor can't save the form, when it can't.
    pub(crate) fn draft(self) -> Result<ReleaseEditDraft, DbError> {
        let Self { session, artist_resolutions, album, cover_version, tracks, has_tracks, files } =
            self;
        let StoredReleaseEdit { release_id, opened, form, applied, album_pick } = session;
        let changed = form != opened.edit || applied.is_some() || album_pick != AlbumPick::Records;
        let unreadable = |error: crate::ReleaseEditLoadError| {
            DbError::unreadable(format!("the tracks of release {release_id}"), error)
        };
        let rows = tracks
            .into_iter()
            .map(|track| track.table_row(&unreadable))
            .collect::<Result<Vec<_>, _>>()?;
        let invalid = form.shape().err();
        Ok(ReleaseEditDraft {
            edit: form,
            cover: bae_records::cover_ref(&release_id, cover_version),
            source_audio: files.source_audio.clone(),
            sides: has_tracks.then(|| crate::ReleaseEditTrackSide::lay_out(rows)),
            files: files
                .has_others
                .then(|| crate::ReleaseEditDisplayContext::carried_files(&files)),
            album,
            artist_resolutions,
            changed,
            invalid,
        })
    }
}
