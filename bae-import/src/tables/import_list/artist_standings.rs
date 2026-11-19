//! How each artist field of a candidate's draft or of a release editor's
//! session stands to the library: the album's artists, and each track's own.
//! One set of WITH clauses over the field's credits reads it, which the
//! statements of the pane and the editor put in front of their own and join
//! to the rows they read.
//!
//! The clauses read which library artists each credit names from the answer
//! the library stores for it (`credit_resolutions`), which every write that
//! can change the answer stores in its own transaction. A picked artist is in
//! the library. A field's own standing counts its artists' with GROUP BY.

use super::*;
use crate::{ArtistFieldStanding, ArtistStanding, ArtistsStanding};
use bae_records::ExistingArtist;

/// The WITH clauses that end in `artist_field_standing (part, track_id,
/// standing)`: one row per artist field the source's statement
/// ([`CreditSource::raw`]) selects a credit of, the album's in part 0 and
/// each explicit track's in part 1 by its track id, with the field's
/// standing as JSON ([`artist_field_standing_of_json`]).
const ARTIST_STANDING_CLAUSES: &str = "
raw AS ({raw}),
-- Each place of each field with its artist's standing: a picked artist is in
-- the library, and a credit stands as its stored answer says, with the
-- library artists it could be when it is ambiguous. The answer to a credit
-- is the library artists it names, in the rule's order: none for a new
-- artist, one in the library, several left ambiguous. A credit with no
-- stored answer has no kind, which no standing reads.
placed_standing AS (
    SELECT part, track_id, position, kind,
           CASE WHEN kind = 'ambiguous' THEN artists END AS choices
    FROM (SELECT raw.part, raw.track_id, raw.position,
                 CASE WHEN raw.assignment_kind = 'picked' THEN 'library'
                      WHEN answer.id IS NULL THEN NULL
                      ELSE CASE (SELECT COUNT(*) FROM import_credit_resolution_artist AS a
                                 WHERE a.resolution = answer.id)
                               WHEN 0 THEN 'new' WHEN 1 THEN 'library' ELSE 'ambiguous' END
                 END AS kind,
                 (SELECT json_group_array(json_object(
                             'artist_id', a.artist_id, 'name', a.name, 'sort_name', a.sort_name,
                             'musicbrainz_artist_id', a.musicbrainz_artist_id,
                             'discogs_artist_id', a.discogs_artist_id) ORDER BY a.position)
                  FROM import_credit_resolution_artist AS a
                  WHERE a.resolution = answer.id) AS artists
          FROM raw
          LEFT JOIN import_credit_resolution AS answer
              ON raw.assignment_kind = 'credit'
             AND answer.name_key = bae_artist_name_key(raw.name)
             AND answer.musicbrainz_artist_id IS raw.mb AND answer.discogs_artist_id IS raw.dg)
),
-- Each field's standing, counted over its places: one artist that could be
-- several is a choice among them; else any such artist leaves that many to
-- choose; else the field is new, in the library, or partly new.
field_counts AS (
    SELECT part, track_id, COUNT(*) AS total,
           SUM(kind = 'ambiguous') AS to_choose, SUM(kind = 'new') AS new_count,
           MAX(json_array_length(choices)) AS choice_count,
           json_group_array(json_object('kind', kind, 'choices', json(choices))
                            ORDER BY position) AS artists
    FROM placed_standing GROUP BY part, track_id
),
artist_field_standing AS MATERIALIZED (
    SELECT part, track_id,
           json_object(
               'kind', CASE WHEN total = 1 AND to_choose = 1 THEN 'choose'
                            WHEN to_choose > 0 THEN 'some_to_choose'
                            WHEN new_count = total THEN 'new'
                            WHEN new_count = 0 THEN 'library'
                            ELSE 'some_new' END,
               'count', CASE WHEN total = 1 AND to_choose = 1 THEN choice_count
                             WHEN to_choose > 0 THEN to_choose
                             WHEN new_count = total OR new_count = 0 THEN NULL
                             ELSE new_count END,
               'artists', json(artists)) AS standing
    FROM field_counts
)";

/// Whose artist fields the standing clauses read: a candidate's draft, or a
/// release editor's session by its id.
#[derive(Clone, Copy)]
pub(crate) enum CreditSource<'a> {
    Candidate(crate::tables::import_state::OneCandidate<'a>),
    Session(&'a i64),
}

impl CreditSource<'_> {
    /// The statement selecting each artist of the source's album artists and
    /// explicit tracks' artists as `(part, track_id, track_position,
    /// position, assignment_kind, credited_as, name, sort_name, mb, dg)`: the
    /// album's in part 0 with no track, each track's in part 1.
    fn raw(self) -> String {
        let (tables, track, owner) = match self {
            Self::Candidate(candidate) => (
                &crate::tables::import_state::CANDIDATE_DRAFT,
                "import_candidate_track",
                candidate.hash_sql(),
            ),
            Self::Session(_) => {
                (&crate::tables::RELEASE_EDIT_SESSION_DRAFT, "release_edit_track", ":session")
            }
        };
        format!(
            "SELECT 0 AS part, NULL AS track_id, 0 AS track_position, a.position AS position, \
                    a.assignment_kind, a.credited_as, a.name, a.sort_name, \
                    a.musicbrainz_artist_id AS mb, a.discogs_artist_id AS dg \
             FROM {album_artist} AS a \
             WHERE a.{key} = {owner} \
             UNION ALL \
             SELECT 1, t.track_id, t.position, a.position, a.assignment_kind, a.credited_as, \
                    a.name, a.sort_name, a.musicbrainz_artist_id, a.discogs_artist_id \
             FROM {track_artist} AS a \
             JOIN {track} AS t ON t.{key} = a.{key} AND t.track_id = a.track_id \
             WHERE a.{key} = {owner} AND t.artist_assignment_kind = 'explicit'",
            album_artist = tables.album_artist,
            track_artist = tables.track_artist,
            key = tables.key,
        )
    }

    /// What [`Self::raw`] binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match self {
            Self::Candidate(candidate) => candidate.params(),
            Self::Session(session) => vec![(":session", *session as &dyn rusqlite::ToSql)],
        }
    }
}

/// The clauses over `source`'s credits. A statement puts them after its
/// WITH and binds [`CreditSource::params`].
pub(crate) fn artist_standing_clauses(source: CreditSource<'_>) -> String {
    ARTIST_STANDING_CLAUSES.replace("{raw}", &source.raw())
}

/// An expression for the standing JSON of the artist field of the track
/// whose id `track_id` holds, in a statement that puts
/// [`artist_standing_clauses`] after its WITH; NULL when the track takes its
/// album's artists or has none of its own.
pub(crate) fn track_artist_standing_sql(track_id: &str) -> String {
    format!(
        "(SELECT standing FROM artist_field_standing \
          WHERE part = 1 AND track_id = {track_id})"
    )
}

/// How the album artists of `source` stand to the library; `None` when the
/// album has no artists.
pub(crate) fn album_artist_standing_on(
    sql: &impl coven::SqlRead,
    source: CreditSource<'_>,
) -> Result<Option<ArtistFieldStanding>, DbError> {
    let json: Option<String> = sql
        .query_row(
            &format!(
                "WITH {} SELECT standing FROM artist_field_standing WHERE part = 0",
                artist_standing_clauses(source)
            ),
            source.params().as_slice(),
            |row| row.get(0),
        )
        .optional()?;
    json.as_deref().map(artist_field_standing_of_json).transpose()
}

/// One artist's standing as the clauses write it.
#[derive(serde::Deserialize)]
struct ArtistStandingJson {
    kind: String,
    choices: Option<Vec<ExistingArtist>>,
}

/// One field's standing as the clauses write it.
#[derive(serde::Deserialize)]
pub(crate) struct FieldStandingJson {
    kind: String,
    count: Option<u32>,
    artists: Vec<ArtistStandingJson>,
}

impl FieldStandingJson {
    /// The standing the clauses wrote, or what in it no clause writes.
    pub(crate) fn standing(self) -> Result<ArtistFieldStanding, String> {
        let field = match (self.kind.as_str(), self.count) {
            ("library", None) => ArtistsStanding::Library,
            ("new", None) => ArtistsStanding::New,
            ("some_new", Some(count)) => ArtistsStanding::SomeNew { count },
            ("choose", Some(choices)) => ArtistsStanding::Choose { choices },
            ("some_to_choose", Some(count)) => ArtistsStanding::SomeToChoose { count },
            (kind, count) => return Err(format!("a field standing {kind} of count {count:?}")),
        };
        let artists = self
            .artists
            .into_iter()
            .map(|artist| match (artist.kind.as_str(), artist.choices) {
                ("library", None) => Ok(ArtistStanding::Library),
                ("new", None) => Ok(ArtistStanding::New),
                ("ambiguous", Some(choices)) => Ok(ArtistStanding::Choose { choices }),
                (kind, choices) => Err(format!(
                    "an artist standing {kind} {} choices",
                    if choices.is_some() { "with" } else { "without" }
                )),
            })
            .collect::<Result<_, _>>()?;
        Ok(ArtistFieldStanding { field, artists })
    }
}

/// The field standing the clauses wrote as `json`.
pub(crate) fn artist_field_standing_of_json(json: &str) -> Result<ArtistFieldStanding, DbError> {
    let stated: FieldStandingJson =
        serde_json::from_str(json).map_err(|source| DbError::UnreadableJson {
            what: "an artist field's standing".to_string(),
            json: json.to_string(),
            source,
        })?;
    stated.standing().map_err(|stated| DbError::unreadable("an artist field's standing", stated))
}

#[cfg(test)]
#[path = "artist_standings_tests.rs"]
mod tests;
