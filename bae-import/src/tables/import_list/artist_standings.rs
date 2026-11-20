//! Each artist field of a candidate's draft or of a release editor's form
//! (the album's artists, and each track's own) with how each of its credits
//! and the whole field stand to the library. One set of WITH clauses over the
//! field's credits reads each credit together with its standing, which the
//! statements of the pane and the editor put in front of their own and join
//! to the rows they read.
//!
//! The clauses read which library artists each credit names from the records'
//! rule's answer to it: for a draft, the answer the library stores
//! (`credit_resolutions`), which every write that can change it stores in its
//! own transaction; for a form, which no table holds because what the person
//! types is view state, the answer the rule's SQL
//! (`bae_records::credit_artist_clauses`) gives in the same statement. A
//! picked artist is in the library. A field's own standing counts its
//! artists' with GROUP BY.

use super::*;
use crate::tables::draft_rows::{assignment_joins_sql, StoredAssignment, ASSIGNMENT_JSON};
use crate::{ArtistField, ArtistStanding, ArtistsStanding, StandingCredit};
use bae_records::ExistingArtist;

/// The WITH clauses that end in `artist_field (part, track_id, field)`: one
/// row per artist field the source's statement ([`CreditSource::raw`])
/// selects a credit of, the album's in part 0 and each explicit track's in
/// part 1 by its track id, with the field's credits and standings as JSON
/// ([`artist_field_of_json`]).
const ARTIST_FIELD_CLAUSES: &str = "
raw AS ({raw}),
-- The rule's answer to each credit the source answers, by the folded name
-- and catalog ids the rule reads: the library artists it names as a JSON
-- array, in the rule's order, in `credit_answer (name_key, mb, dg, artists)`.
{answers},
-- Each place of each field with its credit and its artist's standing: a
-- picked artist is in the library, and a credit stands as its answer says,
-- with the library artists it could be when it is ambiguous: none names a
-- new artist, one an artist in the library, several leave it ambiguous. A
-- credit with no answer has no kind, which no standing reads.
placed_standing AS (
    SELECT part, track_id, position, credit, kind,
           CASE WHEN kind = 'ambiguous' THEN artists END AS choices
    FROM (SELECT assignment.part, assignment.track_id, assignment.position,
                 {credit} AS credit,
                 CASE WHEN assignment.assignment_kind = 'picked' THEN 'library'
                      WHEN answer.name_key IS NULL THEN NULL
                      ELSE CASE json_array_length(answer.artists)
                               WHEN 0 THEN 'new' WHEN 1 THEN 'library' ELSE 'ambiguous' END
                 END AS kind,
                 answer.artists AS artists
          FROM raw AS assignment
          {joins}
          LEFT JOIN credit_answer AS answer
              ON assignment.assignment_kind = 'credit'
             AND answer.name_key = bae_artist_name_key(assignment.name)
             AND answer.mb IS assignment.musicbrainz_artist_id
             AND answer.dg IS assignment.discogs_artist_id)
),
-- Each field's standing, counted over its places: one artist that could be
-- several is a choice among them; else any such artist leaves that many to
-- choose; else the field is new, in the library, or partly new.
field_counts AS (
    SELECT part, track_id, COUNT(*) AS total,
           SUM(kind = 'ambiguous') AS to_choose, SUM(kind = 'new') AS new_count,
           MAX(json_array_length(choices)) AS choice_count,
           json_group_array(json_object('credit', json(credit), 'kind', kind,
                                        'choices', json(choices))
                            ORDER BY position) AS credits
    FROM placed_standing GROUP BY part, track_id
),
artist_field AS MATERIALIZED (
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
               'credits', json(credits)) AS field
    FROM field_counts
)";

/// Whose artist fields the standing clauses read: a candidate's draft, or a
/// release editor's form, whose album artists and explicit tracks' artists
/// the statement binds as the JSON arrays that the release editor's
/// `assignment_rows` makes.
#[derive(Clone, Copy)]
pub(crate) enum CreditSource<'a> {
    Candidate(crate::tables::import_state::OneCandidate<'a>),
    Form { album_artists: &'a String, track_artists: &'a String },
}

/// The columns of a draft's assignment table that [`CreditSource::raw`]
/// selects from the assignment rows aliased `a`.
const ASSIGNMENT_COLUMNS: &str = "a.assignment_kind, a.artist_id, a.name, a.sort_name, \
                                  a.musicbrainz_artist_id, a.discogs_artist_id, a.credit_key, \
                                  a.credited_as, a.join_phrase";

impl CreditSource<'_> {
    /// The statement selecting each artist of the source's album artists and
    /// explicit tracks' artists as `(part, track_id, track_position,
    /// position)` and the columns of a draft's assignment table: the album's
    /// in part 0 with no track, each track's in part 1.
    fn raw(self) -> String {
        match self {
            Self::Candidate(candidate) => {
                format!(
                    "SELECT 0 AS part, NULL AS track_id, 0 AS track_position, \
                            a.position AS position, {ASSIGNMENT_COLUMNS} \
                     FROM import_candidate_album_artist_assignment AS a \
                     WHERE a.content_hash = {owner} \
                     UNION ALL \
                     SELECT 1, t.track_id, t.position, a.position, {ASSIGNMENT_COLUMNS} \
                     FROM import_candidate_track_artist_assignment AS a \
                     JOIN import_candidate_track AS t \
                         ON t.content_hash = a.content_hash AND t.track_id = a.track_id \
                     WHERE a.content_hash = {owner} AND t.artist_assignment_kind = 'explicit'",
                    owner = candidate.hash_sql(),
                )
            }
            Self::Form { .. } => {
                let rows = crate::tables::release_edit::assignment_rows_sql;
                format!(
                    "SELECT 0 AS part, NULL AS track_id, 0 AS track_position, \
                            a.position AS position, {ASSIGNMENT_COLUMNS} \
                     FROM {album} AS a \
                     UNION ALL \
                     SELECT 1, a.track_id, a.track_position, a.position, {ASSIGNMENT_COLUMNS} \
                     FROM {tracks} AS a",
                    album = rows(":album_artists"),
                    tracks = rows(":track_artists"),
                )
            }
        }
    }

    /// The clauses that end in `credit_answer (name_key, mb, dg, artists)`,
    /// the rule's answer to each credit: for a candidate, the answers the
    /// library stores for its draft's credits (`credit_resolutions`); for a
    /// form, the rule's SQL over the form's credits in `raw`.
    fn answers(self) -> String {
        use crate::tables::credit_resolutions::{rule_answer_json_sql, stored_answer_json_sql};
        match self {
            Self::Candidate(_) => format!(
                "credit_answer (name_key, mb, dg, artists) AS ( \
                     SELECT r.name_key, r.musicbrainz_artist_id, r.discogs_artist_id, {} \
                     FROM import_credit_resolution AS r)",
                stored_answer_json_sql("r.id"),
            ),
            Self::Form { .. } => format!(
                "form_credit AS ( \
                     SELECT DISTINCT bae_artist_name_key(name) AS name_key, \
                            musicbrainz_artist_id, discogs_artist_id \
                     FROM raw WHERE assignment_kind = 'credit'), \
                 {rule}, \
                 credit_answer (name_key, mb, dg, artists) AS ( \
                     SELECT c.name_key, c.musicbrainz_artist_id, c.discogs_artist_id, {answer} \
                     FROM form_credit AS c)",
                rule = bae_records::credit_artist_clauses("form_credit"),
                answer = rule_answer_json_sql("c"),
            ),
        }
    }

    /// What [`Self::raw`] binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match self {
            Self::Candidate(candidate) => candidate.params(),
            Self::Form { album_artists, track_artists } => vec![
                (":album_artists", *album_artists as &dyn rusqlite::ToSql),
                (":track_artists", *track_artists as &dyn rusqlite::ToSql),
            ],
        }
    }
}

/// The clauses over `source`'s credits. A statement puts them after its
/// WITH and binds [`CreditSource::params`].
pub(crate) fn artist_field_clauses(source: CreditSource<'_>) -> String {
    ARTIST_FIELD_CLAUSES
        .replace("{raw}", &source.raw())
        .replace("{answers}", &source.answers())
        .replace("{credit}", ASSIGNMENT_JSON)
        .replace("{joins}", &assignment_joins_sql())
}

/// An expression for the JSON of the artist field of the track whose id
/// `track_id` holds, in a statement that puts [`artist_field_clauses`] after
/// its WITH; NULL when the track takes its album's artists or has none of
/// its own.
pub(crate) fn track_artist_field_sql(track_id: &str) -> String {
    format!("(SELECT field FROM artist_field WHERE part = 1 AND track_id = {track_id})")
}

/// The album artists of `source`, each with how it stands to the library;
/// `None` when the album has no artists.
pub(crate) fn album_artist_field_on(
    sql: &impl coven::SqlRead,
    source: CreditSource<'_>,
) -> Result<Option<ArtistField>, DbError> {
    let json: Option<String> = sql
        .query_row(
            &format!(
                "WITH {} SELECT field FROM artist_field WHERE part = 0",
                artist_field_clauses(source)
            ),
            source.params().as_slice(),
            |row| row.get(0),
        )
        .optional()?;
    json.as_deref().map(artist_field_of_json).transpose()
}

/// One credit of a field with its artist's standing, as the clauses write it.
#[derive(serde::Deserialize)]
struct StandingCreditJson {
    credit: StoredAssignment,
    kind: String,
    choices: Option<Vec<ExistingArtist>>,
}

/// One field as the clauses write it.
#[derive(serde::Deserialize)]
pub(crate) struct ArtistFieldJson {
    kind: String,
    count: Option<u32>,
    credits: Vec<StandingCreditJson>,
}

impl ArtistFieldJson {
    /// The field the clauses wrote, or what in it no clause writes.
    pub(crate) fn field(self) -> Result<ArtistField, DbError> {
        let unreadable = |stated: String| DbError::unreadable("an artist field's standing", stated);
        let standing = match (self.kind.as_str(), self.count) {
            ("library", None) => ArtistsStanding::Library,
            ("new", None) => ArtistsStanding::New,
            ("some_new", Some(count)) => ArtistsStanding::SomeNew { count },
            ("choose", Some(choices)) => ArtistsStanding::Choose { choices },
            ("some_to_choose", Some(count)) => ArtistsStanding::SomeToChoose { count },
            (kind, count) => {
                return Err(unreadable(format!("a field standing {kind} of count {count:?}")))
            }
        };
        let credits = self
            .credits
            .into_iter()
            .map(|credit| {
                let standing = match (credit.kind.as_str(), credit.choices) {
                    ("library", None) => ArtistStanding::Library,
                    ("new", None) => ArtistStanding::New,
                    ("ambiguous", Some(choices)) => ArtistStanding::Choose { choices },
                    (kind, choices) => {
                        return Err(unreadable(format!(
                            "an artist standing {kind} {} choices",
                            if choices.is_some() { "with" } else { "without" }
                        )))
                    }
                };
                Ok(StandingCredit { credited: credit.credit.credited()?, standing })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(ArtistField { standing, credits })
    }
}

/// The field the clauses wrote as `json`.
pub(crate) fn artist_field_of_json(json: &str) -> Result<ArtistField, DbError> {
    let stated: ArtistFieldJson =
        serde_json::from_str(json).map_err(|source| DbError::UnreadableJson {
            what: "an artist field".to_string(),
            json: json.to_string(),
            source,
        })?;
    stated.field()
}

#[cfg(test)]
#[path = "artist_standings_tests.rs"]
mod tests;
