//! What the library holds for each artist credit of a candidate's draft or
//! of a release editor's session, which one statement over its credits
//! reads.
//!
//! The statement states the rule an import commits by (`bae_records`'
//! `artist_resolution` and `catalog_match`) as a read of the credits alone, in
//! SQL: a credit's catalog ids name the library artist holding them, followed
//! through merges to the smallest id and filled with the ids and sort name the
//! other holders have; when the ids name different artists, the statement
//! takes the artist the MusicBrainz id names; Various Artists is one artist
//! in both catalogs; a credit no id places is the one library artist of its
//! folded name, or ambiguous between several.
//! `credit_resolutions_tests` runs the statement and the rule's own function
//! over the same credits and libraries and requires the same answers.

use super::*;
use bae_records::{CreditResolution, ExistingArtist, ResolvedCredit};

/// The resolution of every distinct credit that the source's statement
/// ([`CreditSource::raw`]) selects, in the order the draft lists them: its
/// album's credits, then each explicit track's.
///
/// SQLite copies a WITH name's whole definition at every place that names it,
/// and a statement holding more than `SQLITE_LIMIT_EXPR_DEPTH` (1000) cursors
/// in all stops with "VIEWs and/or subqueries nested too deep" (as
/// `cards_sql` explains). So the statement names each WITH name below only
/// once, collects the artists holding a catalog id into one JSON column, and
/// puts the four tries of a credit's ids in columns of one row.
const CREDIT_RESOLUTIONS: &str = "
WITH
raw AS ({raw}),
credit AS (
    SELECT ROW_NUMBER() OVER (ORDER BY first_seen) AS idx, credited_as, name, sort_name, mb, dg,
           bae_artist_name_key(name) AS name_key
    FROM (SELECT credited_as, name, sort_name, mb, dg,
                 MIN(part * 1000000000000 + track_position * 1000000 + position) AS first_seen
          FROM raw GROUP BY credited_as, name, sort_name, mb, dg)
),
-- The ids a credit is tried by: as it is (0); with the other catalog's Various
-- Artists id beside the one it names (1); and each by its MusicBrainz id alone
-- (2, 3); the statement uses these tries when the ids name different
-- artists. For each try, the statement collects the shown artists holding
-- its Discogs id and its MusicBrainz id, or named by the id either derives,
-- into JSON arrays in id order.
stage AS (
    SELECT t.*,
           CASE WHEN t.dg IS NULL THEN '[]' ELSE (
               SELECT json_group_array(json_array(a.id, a.discogs_artist_id,
                                                  a.musicbrainz_artist_id, a.sort_name)
                                       ORDER BY a.id)
               FROM artists AS a
               WHERE a.id IN (SELECT shown.shown_artist_id FROM artists AS named
                              JOIN artist_shown_as AS shown ON shown.artist_id = named.id
                              WHERE named.discogs_artist_id = t.dg
                                 OR named.id = bae_artist_identity(NULL, t.dg))) END AS held_d,
           CASE WHEN t.mb IS NULL THEN '[]' ELSE (
               SELECT json_group_array(json_array(a.id, a.discogs_artist_id,
                                                  a.musicbrainz_artist_id, a.sort_name)
                                       ORDER BY a.id)
               FROM artists AS a
               WHERE a.id IN (SELECT shown.shown_artist_id FROM artists AS named
                              JOIN artist_shown_as AS shown ON shown.artist_id = named.id
                              WHERE named.musicbrainz_artist_id = t.mb
                                 OR named.id = bae_artist_identity(t.mb, NULL))) END AS held_m
    FROM (SELECT c.idx, c.credited_as, c.name, c.sort_name, c.mb AS cmb, c.dg AS cdg,
                 c.name_key, k.stage,
                 CASE WHEN k.stage IN (1, 3) AND c.mb IS NULL AND c.dg = '{VA_DISCOGS}'
                      THEN '{VA_MUSICBRAINZ}' ELSE c.mb END AS mb,
                 CASE k.stage
                     WHEN 0 THEN c.dg
                     WHEN 1 THEN CASE WHEN c.dg IS NULL AND c.mb = '{VA_MUSICBRAINZ}'
                                      THEN '{VA_DISCOGS}' ELSE c.dg END
                 END AS dg
          FROM credit AS c
          CROSS JOIN (SELECT 0 AS stage UNION ALL SELECT 1 UNION ALL SELECT 2
                      UNION ALL SELECT 3) AS k
          WHERE k.stage IN (0, 2)
             OR (c.mb IS NULL AND c.dg = '{VA_DISCOGS}')
             OR (c.dg IS NULL AND c.mb = '{VA_MUSICBRAINZ}')) AS t
),
-- What the catalog ids of each try say: no artist holds them, one does, or
-- they disagree: the artist one id names holds another id of the other
-- catalog, or the two ids name two artists. The smallest id holding an id is
-- the artist, which takes the ids and sort name it lacks from the others, in
-- id order.
outcome AS (
    SELECT x.idx, x.credited_as, x.name, x.sort_name, x.cmb, x.cdg, x.name_key, x.stage,
           x.mb, x.dg,
           CASE
             WHEN x.d_holder IS NULL AND x.m_holder IS NULL THEN 'unmatched'
             WHEN x.d_holder IS NULL THEN
                 CASE WHEN {agree_m} THEN 'found' ELSE 'disagrees' END
             WHEN x.m_holder IS NULL THEN
                 CASE WHEN {agree_d} THEN 'found' ELSE 'disagrees' END
             WHEN x.d_holder = x.m_holder THEN
                 CASE WHEN {agree_m} THEN 'found' ELSE 'disagrees' END
             ELSE 'disagrees'
           END AS outcome,
           CASE WHEN x.m_holder IS NOT NULL THEN x.m_holder ELSE x.d_holder END AS holder,
           CASE WHEN x.m_holder IS NOT NULL THEN x.m_dg ELSE x.d_dg END AS dg_f,
           CASE WHEN x.m_holder IS NOT NULL THEN x.m_mb ELSE x.d_mb END AS mb_f,
           CASE WHEN x.m_holder IS NOT NULL THEN x.m_sort ELSE x.d_sort END AS sort_f
    FROM (SELECT y.*,
                 y.held_d ->> '$[0][0]' AS d_holder,
                 (SELECT j.value ->> 1 FROM json_each(y.held_d) AS j
                  WHERE j.value ->> 1 IS NOT NULL ORDER BY j.key LIMIT 1) AS d_dg,
                 (SELECT j.value ->> 2 FROM json_each(y.held_d) AS j
                  WHERE j.value ->> 2 IS NOT NULL ORDER BY j.key LIMIT 1) AS d_mb,
                 (SELECT j.value ->> 3 FROM json_each(y.held_d) AS j
                  WHERE j.value ->> 3 IS NOT NULL ORDER BY j.key LIMIT 1) AS d_sort,
                 y.held_m ->> '$[0][0]' AS m_holder,
                 (SELECT j.value ->> 1 FROM json_each(y.held_m) AS j
                  WHERE j.value ->> 1 IS NOT NULL ORDER BY j.key LIMIT 1) AS m_dg,
                 (SELECT j.value ->> 2 FROM json_each(y.held_m) AS j
                  WHERE j.value ->> 2 IS NOT NULL ORDER BY j.key LIMIT 1) AS m_mb,
                 (SELECT j.value ->> 3 FROM json_each(y.held_m) AS j
                  WHERE j.value ->> 3 IS NOT NULL ORDER BY j.key LIMIT 1) AS m_sort
          FROM stage AS y) AS x
),
-- A credit's four tries as one row.
tries AS (
    SELECT idx, MAX(credited_as) AS credited_as, MAX(name) AS name, MAX(sort_name) AS sort_name,
           MAX(cmb) AS cmb, MAX(cdg) AS cdg, MAX(name_key) AS name_key, {pivot}
    FROM outcome GROUP BY idx
),
-- The try the credit settles on: its ids as they are, unless no artist holds
-- them and the Various Artists ids beside them place it. Then the artist the
-- settled ids name or, where they disagree, the same ids by their MusicBrainz
-- id alone name, and the ids the name step compares an artist's with.
placed AS (
    SELECT q.idx, q.credited_as, q.name, q.sort_name, q.cmb, q.cdg, q.name_key,
           {settled_holder} AS holder, {settled_dg_f} AS dg_f, {settled_mb_f} AS mb_f,
           {settled_sort_f} AS sort_f, {settled_name_mb} AS name_mb,
           {settled_name_dg} AS name_dg
    FROM (SELECT r.*,
                 CASE WHEN r.o0 = 'unmatched' AND r.o1 IS NOT NULL AND r.o1 <> 'unmatched'
                      THEN 1 ELSE 0 END AS pick
          FROM tries AS r) AS q
)
-- A credit no id places is the library artist of its folded name: an artist
-- goes by its own name and by each other printed name, an artist a merge
-- absorbed by the artist it shows as, and no catalog id the credit and the
-- artist both carry may differ.
SELECT p.credited_as, p.name, p.sort_name, p.cmb AS mb, p.cdg AS dg,
       CASE WHEN p.holder IS NOT NULL THEN 'library'
            ELSE CASE json_array_length(p.named)
                      WHEN 0 THEN 'new' WHEN 1 THEN 'library' ELSE 'ambiguous' END
       END AS kind,
       CASE WHEN p.holder IS NOT NULL THEN
                (SELECT json_array(json_object(
                     'artist_id', held.id, 'name', held.name, 'sort_name', p.sort_f,
                     'musicbrainz_artist_id', p.mb_f, 'discogs_artist_id', p.dg_f))
                 FROM artists AS held WHERE held.id = p.holder)
            ELSE p.named
       END AS artists
FROM (SELECT z.*,
             (SELECT json_group_array(json_object(
                         'artist_id', a.id, 'name', a.name, 'sort_name', a.sort_name,
                         'musicbrainz_artist_id', a.musicbrainz_artist_id,
                         'discogs_artist_id', a.discogs_artist_id) ORDER BY a.id)
              FROM artists AS a
              WHERE z.holder IS NULL AND z.name_key <> ''
                AND EXISTS (SELECT 1 FROM artist_shown_as AS shown
                            WHERE shown.artist_id = a.id AND shown.shown_artist_id = a.id)
                AND (a.name_key = z.name_key
                     OR a.id IN (SELECT shown.shown_artist_id FROM artist_credits AS printed
                                 JOIN artist_shown_as AS shown
                                     ON shown.artist_id = printed.artist_id
                                 WHERE printed.name_key = z.name_key))
                AND (a.discogs_artist_id IS NULL OR z.name_dg IS NULL
                     OR a.discogs_artist_id = z.name_dg)
                AND (a.musicbrainz_artist_id IS NULL OR z.name_mb IS NULL
                     OR a.musicbrainz_artist_id = z.name_mb)) AS named
      FROM placed AS z) AS p
ORDER BY p.idx";

/// Whose credits a resolution statement reads: a candidate's draft, or a
/// release editor's session by its id.
#[derive(Clone, Copy)]
pub(crate) enum CreditSource<'a> {
    Candidate(crate::tables::import_state::OneCandidate<'a>),
    Session(&'a i64),
}

impl CreditSource<'_> {
    /// The statement selecting each credit of the source's album artists and
    /// explicit tracks' artists as `(part, track_position, position,
    /// credited_as, name, sort_name, mb, dg)`: the album's in part 0, each
    /// track's in part 1.
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
            "SELECT 0 AS part, 0 AS track_position, a.position AS position, a.credited_as, \
                    a.name, a.sort_name, a.musicbrainz_artist_id AS mb, a.discogs_artist_id AS dg \
             FROM {album_artist} AS a \
             WHERE a.{key} = {owner} AND a.assignment_kind = 'credit' \
             UNION ALL \
             SELECT 1, t.position, a.position, a.credited_as, a.name, a.sort_name, \
                    a.musicbrainz_artist_id, a.discogs_artist_id \
             FROM {track_artist} AS a \
             JOIN {track} AS t ON t.{key} = a.{key} AND t.track_id = a.track_id \
             WHERE a.{key} = {owner} AND a.assignment_kind = 'credit' \
               AND t.artist_assignment_kind = 'explicit'",
            album_artist = tables.album_artist,
            track_artist = tables.track_artist,
            key = tables.key,
        )
    }

    /// What [`Self::raw`] binds, as a statement's parameters.
    fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match self {
            Self::Candidate(candidate) => candidate.params(),
            Self::Session(session) => vec![(":session", *session as &dyn rusqlite::ToSql)],
        }
    }
}

/// The statement over `source`'s credits with the ids it names: the Various
/// Artists entries of both catalogs, how a try's ids agree with the ids an
/// artist holds, and a column of the credit's row for each column of each
/// try.
fn statement(source: CreditSource<'_>) -> String {
    // Whether no catalog id the try's credit carries differs from the one the
    // holder's columns (`dg`, `mb` prefixed by `side`) have, once filled.
    let agree = |side: &str| {
        format!(
            "(x.{side}_dg IS NULL OR x.dg IS NULL OR x.{side}_dg = x.dg) \
             AND (x.{side}_mb IS NULL OR x.mb IS NULL OR x.{side}_mb = x.mb)"
        )
    };
    // One column of each of the four tries.
    let pivot = (0..4)
        .flat_map(|stage| {
            [
                ("o", "outcome"),
                ("h", "holder"),
                ("d", "dg_f"),
                ("m", "mb_f"),
                ("s", "sort_f"),
                ("i", "mb"),
                ("j", "dg"),
            ]
            .map(move |(prefix, column)| {
                format!("MAX(CASE WHEN stage = {stage} THEN {column} END) AS {prefix}{stage}")
            })
        })
        .collect::<Vec<_>>()
        .join(", ");
    // A column of the try the credit settled on, and of the try by its
    // MusicBrainz id alone that follows it.
    let chosen = |prefix: &str| format!("CASE q.pick WHEN 1 THEN q.{prefix}1 ELSE q.{prefix}0 END");
    let alone = |prefix: &str| format!("CASE q.pick WHEN 1 THEN q.{prefix}3 ELSE q.{prefix}2 END");
    // The artist the settled try names: its own, or, where its ids disagree,
    // the one its MusicBrainz id alone names.
    let settled = |prefix: &str| {
        format!(
            "CASE {outcome} WHEN 'found' THEN {chosen} \
                 WHEN 'disagrees' THEN (CASE WHEN {alone_outcome} = 'found' THEN {alone} END) END",
            outcome = chosen("o"),
            chosen = chosen(prefix),
            alone_outcome = alone("o"),
            alone = alone(prefix),
        )
    };
    CREDIT_RESOLUTIONS
        .replace("{raw}", &source.raw())
        .replace("{VA_DISCOGS}", bae_records::VARIOUS_ARTISTS.discogs)
        .replace("{VA_MUSICBRAINZ}", bae_records::VARIOUS_ARTISTS.musicbrainz)
        .replace("{agree_m}", &agree("m"))
        .replace("{agree_d}", &agree("d"))
        .replace("{pivot}", &pivot)
        .replace("{settled_holder}", &settled("h"))
        .replace("{settled_dg_f}", &settled("d"))
        .replace("{settled_mb_f}", &settled("m"))
        .replace("{settled_sort_f}", &settled("s"))
        .replace("{settled_name_mb}", &chosen("i"))
        .replace(
            "{settled_name_dg}",
            &format!("CASE {} WHEN 'disagrees' THEN NULL ELSE {} END", chosen("o"), chosen("j")),
        )
}

/// What the library holds for each distinct artist credit of `source`, in
/// the order its draft lists them.
pub(crate) fn credit_resolutions_on(
    sql: &impl coven::SqlRead,
    source: CreditSource<'_>,
) -> Result<Vec<ResolvedCredit>, DbError> {
    Ok(sql.query(&statement(source), source.params().as_slice(), |row| {
        let unreadable = |error: String| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, error.into())
        };
        let artists: Vec<ExistingArtist> = match row.get::<_, Option<String>>("artists")? {
            Some(json) => {
                serde_json::from_str(&json).map_err(|error| unreadable(error.to_string()))?
            }
            None => Vec::new(),
        };
        let resolution = match (row.get::<_, String>("kind")?.as_str(), artists.len()) {
            ("new", 0) => CreditResolution::New,
            ("library", 1) => CreditResolution::Library {
                artist: artists.into_iter().next().expect("one artist"),
            },
            ("ambiguous", count) if count > 1 => CreditResolution::Ambiguous { artists },
            (kind, count) => {
                return Err(unreadable(format!("a {kind} credit resolution with {count} artists")))
            }
        };
        Ok(ResolvedCredit {
            credit: bae_catalogs::credits::ArtistCredit {
                credited_as: row.get("credited_as")?,
                name: row.get("name")?,
                sort_name: row.get("sort_name")?,
                musicbrainz_artist_id: row.get("mb")?,
                discogs_artist_id: row.get("dg")?,
            },
            resolution,
        })
    })?)
}

#[cfg(test)]
#[path = "credit_resolutions_tests.rs"]
mod tests;
