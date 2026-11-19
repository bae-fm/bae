//! SQL functions that compute an artist credit's name key and catalog id with
//! the rules the write code uses, for reads that apply the same rule in SQL.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_artist_name_key(name)`: the folded name that lookups match an artist
/// on (`bae_types::text_match::artist_name_key`); NULL stays NULL. Once an
/// index or trigger uses this function, it is part of the schema: what it
/// folds must not change.
pub(crate) static ARTIST_NAME_KEY: SchemaFunction = SchemaFunction {
    name: "bae_artist_name_key",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        Ok(match arguments.get::<Option<String>>(0)? {
            Some(name) => Value::Text(bae_types::text_match::artist_name_key(&name)),
            None => Value::Null,
        })
    },
};

/// `bae_artist_identity(musicbrainz_id, discogs_id)`: the bae artist id
/// derived from the MusicBrainz id, else from the Discogs id
/// (`crate::identity::artist_id`); NULL when neither id is given.
pub(crate) static ARTIST_IDENTITY: SchemaFunction = SchemaFunction {
    name: "bae_artist_identity",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let musicbrainz = arguments.get::<Option<String>>(0)?;
        let discogs = arguments.get::<Option<String>>(1)?;
        Ok(match crate::identity::artist_id(musicbrainz.as_deref(), discogs.as_deref()) {
            Some(id) => Value::Text(id),
            None => Value::Null,
        })
    },
};

/// `bae_artist_is_various_artists(musicbrainz_id, discogs_id)`: 1 when either
/// id is its catalog's Various Artists entry (`crate::VARIOUS_ARTISTS`),
/// which the rule takes as one artist in both catalogs; else 0.
pub(crate) static ARTIST_IS_VARIOUS_ARTISTS: SchemaFunction = SchemaFunction {
    name: "bae_artist_is_various_artists",
    arguments: 2,
    version: 1,
    compute: |arguments| {
        let musicbrainz = arguments.get::<Option<String>>(0)?;
        let discogs = arguments.get::<Option<String>>(1)?;
        Ok(Value::Integer(i64::from(
            crate::VARIOUS_ARTISTS.named_by(musicbrainz.as_deref(), discogs.as_deref()),
        )))
    },
};

/// The WITH clauses that apply the rule every write of an artist credit
/// decides by (`artist_resolution`) to each credit of the relation `credits`,
/// against the committed library, and end in
/// `credit_artist (name_key, musicbrainz_artist_id, discogs_artist_id,
/// position, artist_id, name, sort_name, artist_musicbrainz_id,
/// artist_discogs_id)`: for each credit `credits` names by its folded name
/// and its two catalog ids, the library artists the rule names for it, from
/// position 0. A credit with no row would commit as a new artist; one row is
/// the artist it names; several are the artists that carry its name when no
/// catalog id tells them apart, so committing it makes a new artist unless
/// the person picks one.
///
/// A statement puts the clauses after its WITH, after the clause that
/// defines `credits` with the columns `name_key`, `musicbrainz_artist_id`
/// and `discogs_artist_id`. The import stores this answer for each credit of
/// a draft, and a release editor's draft, whose form no table holds, reads
/// it for each credit of the form. Their own clauses' names start with
/// `rule_`.
pub fn credit_artist_clauses(credits: &str) -> String {
    // Each clause names the ones before it as few times as it can: SQLite
    // expands every name a clause gives into a copy of that clause, so a
    // clause named twice by each of the clauses after it grows the
    // statement past what SQLite parses.
    let literal = |value: &str| format!("'{}'", value.replace('\'', "''"));
    let va_mb = literal(crate::VARIOUS_ARTISTS.musicbrainz);
    let va_dg = literal(crate::VARIOUS_ARTISTS.discogs);
    let shown_named = super::shown_artist_id("named_id");
    let shown_credit = super::shown_artist_id("credit_name.artist_id");
    let shown = super::artist_is_shown("a");
    format!(
        "rule_credit AS MATERIALIZED (
    SELECT DISTINCT name_key, musicbrainz_artist_id AS mb, discogs_artist_id AS dg,
           -- A credit naming Various Artists by one catalog's id alone goes by
           -- both catalogs' Various Artists ids when no artist holds its own.
           bae_artist_is_various_artists(musicbrainz_artist_id, discogs_artist_id) = 1
               AND (musicbrainz_artist_id IS NULL) <> (discogs_artist_id IS NULL)
               AS various
    FROM {credits}
),
-- Every pair of catalog ids the rule looks up for a credit: its own, the
-- Various Artists pair, and the MusicBrainz id alone, by which a credit
-- whose two ids disagree goes.
rule_probe AS MATERIALIZED (
    SELECT DISTINCT
           CASE look.kind WHEN 'own' THEN c.mb WHEN 'alone' THEN c.mb ELSE {va_mb} END AS mb,
           CASE look.kind WHEN 'own' THEN c.dg WHEN 'various' THEN {va_dg} END AS dg
    FROM rule_credit AS c
    JOIN (SELECT 'own' AS kind UNION ALL SELECT 'alone' UNION ALL SELECT 'various'
          UNION ALL SELECT 'various alone') AS look
        ON look.kind = 'own' OR (look.kind = 'alone' AND c.mb IS NOT NULL)
           OR (look.kind IN ('various', 'various alone') AND c.various)
),
-- Each catalog id of those pairs, with the artist id its catalog entry
-- derives.
rule_probe_id AS MATERIALIZED (
    SELECT DISTINCT side.col, CASE side.col WHEN 'mb' THEN p.mb ELSE p.dg END AS cid,
           CASE side.col WHEN 'mb' THEN bae_artist_identity(p.mb, NULL)
                ELSE bae_artist_identity(NULL, p.dg) END AS derived
    FROM rule_probe AS p
    JOIN (SELECT 'mb' AS col UNION ALL SELECT 'dg') AS side
        ON CASE side.col WHEN 'mb' THEN p.mb ELSE p.dg END IS NOT NULL
),
-- The library artists holding each catalog id, or whose own id the catalog
-- entry derives (the column may have been filled in or edited since),
-- each as the artist it shows as after merges.
rule_holding AS MATERIALIZED (
    SELECT DISTINCT p.col, p.cid, a.id, a.name, a.sort_name,
           a.musicbrainz_artist_id AS mb, a.discogs_artist_id AS dg
    FROM (SELECT p.col, p.cid, named.id AS named_id
          FROM rule_probe_id AS p JOIN artists AS named
              ON p.col = 'mb' AND named.musicbrainz_artist_id = p.cid
          UNION SELECT p.col, p.cid, named.id
          FROM rule_probe_id AS p JOIN artists AS named
              ON p.col = 'dg' AND named.discogs_artist_id = p.cid
          UNION SELECT p.col, p.cid, named.id
          FROM rule_probe_id AS p JOIN artists AS named ON named.id = p.derived) AS p
    JOIN artists AS a ON a.id = {shown_named}
),
-- The one artist each catalog id names: of those holding it, the smallest
-- id, which takes each catalog id and the sort name it lacks from the
-- others in id order, since one id is one artist.
rule_held AS MATERIALIZED (
    SELECT col, cid, MIN(id) AS id,
           json_group_array(name ORDER BY id) ->> 0 AS name,
           json_group_array(sort_name ORDER BY id) FILTER (WHERE sort_name IS NOT NULL)
               ->> 0 AS sort_name,
           json_group_array(mb ORDER BY id) FILTER (WHERE mb IS NOT NULL) ->> 0 AS mb,
           json_group_array(dg ORDER BY id) FILTER (WHERE dg IS NOT NULL) ->> 0 AS dg
    FROM rule_holding GROUP BY col, cid
),
-- What each pair's ids name: no artist; the artist they name, the
-- MusicBrainz one's where both name one; or a disagreement, where they name
-- two artists or the artist named holds another id of a catalog.
rule_match AS MATERIALIZED (
    SELECT mb, dg,
           CASE WHEN d_id IS NULL AND m_id IS NULL THEN 'unmatched'
                WHEN d_id IS NOT NULL AND m_id IS NOT NULL AND d_id <> m_id THEN 'disagrees'
                WHEN (artist_mb IS NULL OR mb IS NULL OR artist_mb = mb)
                     AND (artist_dg IS NULL OR dg IS NULL OR artist_dg = dg) THEN 'found'
                ELSE 'disagrees' END AS kind,
           id, name, sort_name, artist_mb, artist_dg
    FROM (SELECT p.mb, p.dg, d.id AS d_id, m.id AS m_id,
                 CASE WHEN m.id IS NOT NULL THEN m.id ELSE d.id END AS id,
                 CASE WHEN m.id IS NOT NULL THEN m.name ELSE d.name END AS name,
                 CASE WHEN m.id IS NOT NULL THEN m.sort_name ELSE d.sort_name END AS sort_name,
                 CASE WHEN m.id IS NOT NULL THEN m.mb ELSE d.mb END AS artist_mb,
                 CASE WHEN m.id IS NOT NULL THEN m.dg ELSE d.dg END AS artist_dg
          FROM rule_probe AS p
          LEFT JOIN rule_held AS d ON d.col = 'dg' AND d.cid = p.dg
          LEFT JOIN rule_held AS m ON m.col = 'mb' AND m.cid = p.mb)
),
-- The artist each credit's catalog ids settle on: the one its ids name (by
-- both Various Artists ids where its own name none and it names Various
-- Artists by one), else, where they disagree, the one its MusicBrainz id
-- alone names. A credit they settle on none goes by its name, with the ids
-- it has left.
rule_settled AS MATERIALIZED (
    SELECT step.name_key, step.mb, step.dg,
           CASE WHEN step.kind = 'found' THEN step.id
                WHEN step.kind = 'disagrees' AND alone.kind = 'found' THEN alone.id END AS id,
           CASE WHEN step.kind = 'found' THEN step.name ELSE alone.name END AS name,
           CASE WHEN step.kind = 'found' THEN step.sort_name ELSE alone.sort_name END
               AS sort_name,
           CASE WHEN step.kind = 'found' THEN step.artist_mb ELSE alone.artist_mb END
               AS artist_mb,
           CASE WHEN step.kind = 'found' THEN step.artist_dg ELSE alone.artist_dg END
               AS artist_dg,
           step.step_mb AS name_mb,
           CASE WHEN step.kind = 'disagrees' THEN NULL ELSE step.step_dg END AS name_dg
    FROM (SELECT x.name_key, x.mb, x.dg,
                 CASE WHEN x.by_va THEN {va_mb} ELSE x.mb END AS step_mb,
                 CASE WHEN x.by_va THEN {va_dg} ELSE x.dg END AS step_dg,
                 CASE WHEN x.by_va THEN x.v_kind ELSE x.o_kind END AS kind,
                 CASE WHEN x.by_va THEN x.v_id ELSE x.o_id END AS id,
                 CASE WHEN x.by_va THEN x.v_name ELSE x.o_name END AS name,
                 CASE WHEN x.by_va THEN x.v_sort_name ELSE x.o_sort_name END AS sort_name,
                 CASE WHEN x.by_va THEN x.v_mb ELSE x.o_mb END AS artist_mb,
                 CASE WHEN x.by_va THEN x.v_dg ELSE x.o_dg END AS artist_dg
          FROM (SELECT c.name_key, c.mb, c.dg,
                       own.kind = 'unmatched' AND COALESCE(various.kind <> 'unmatched', 0)
                           AS by_va,
                       own.kind AS o_kind, own.id AS o_id, own.name AS o_name,
                       own.sort_name AS o_sort_name, own.artist_mb AS o_mb,
                       own.artist_dg AS o_dg,
                       various.kind AS v_kind, various.id AS v_id, various.name AS v_name,
                       various.sort_name AS v_sort_name, various.artist_mb AS v_mb,
                       various.artist_dg AS v_dg
                FROM rule_credit AS c
                JOIN rule_match AS own ON own.mb IS c.mb AND own.dg IS c.dg
                LEFT JOIN rule_match AS various
                    ON c.various AND various.mb = {va_mb} AND various.dg = {va_dg}) AS x)
         AS step
    LEFT JOIN rule_match AS alone
        ON step.kind = 'disagrees' AND alone.mb IS step.step_mb AND alone.dg IS NULL
),
-- Each credit's artists: the one its catalog ids settle on, else the shown
-- artists whose folded name, their own or another printed name of theirs,
-- is the credit's and whose ids agree with the ids the credit has left.
credit_artist (name_key, musicbrainz_artist_id, discogs_artist_id, position, artist_id,
               name, sort_name, artist_musicbrainz_id, artist_discogs_id) AS MATERIALIZED (
    SELECT r.name_key, r.mb, r.dg,
           CASE WHEN r.id IS NOT NULL THEN 0
                ELSE ROW_NUMBER() OVER (PARTITION BY r.name_key, r.mb, r.dg ORDER BY a.id) - 1
           END,
           COALESCE(r.id, a.id), COALESCE(r.name, a.name),
           CASE WHEN r.id IS NOT NULL THEN r.sort_name ELSE a.sort_name END,
           CASE WHEN r.id IS NOT NULL THEN r.artist_mb ELSE a.musicbrainz_artist_id END,
           CASE WHEN r.id IS NOT NULL THEN r.artist_dg ELSE a.discogs_artist_id END
    FROM rule_settled AS r
    LEFT JOIN (SELECT name_key, id AS artist_id FROM artists
               UNION SELECT credit_name.name_key, {shown_credit}
               FROM artist_credits AS credit_name) AS named
        ON r.id IS NULL AND r.name_key <> '' AND named.name_key = r.name_key
    LEFT JOIN artists AS a
        ON a.id = named.artist_id AND {shown}
       AND (a.musicbrainz_artist_id IS NULL OR r.name_mb IS NULL
            OR a.musicbrainz_artist_id = r.name_mb)
       AND (a.discogs_artist_id IS NULL OR r.name_dg IS NULL OR a.discogs_artist_id = r.name_dg)
    WHERE r.id IS NOT NULL OR a.id IS NOT NULL
)"
    )
}
