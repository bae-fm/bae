//! The rule every write of an artist credit decides by (`artist_resolution`),
//! as a fragment for the reads that apply it in SQL: a draft's stored answers
//! and a release editor's form, which no table holds.

sqlive_compose::fragment!(
    CreditAnswers with credits(name_key TEXT, musicbrainz_artist_id TEXT, discogs_artist_id TEXT)
        {
            name_key: Option<String>,
            musicbrainz_artist_id: Option<String>,
            discogs_artist_id: Option<String>,
            artists: String
        },
    r#"
    WITH rule_credit AS MATERIALIZED (
        SELECT DISTINCT name_key, musicbrainz_artist_id AS mb, discogs_artist_id AS dg,
               bae_artist_is_various_artists(musicbrainz_artist_id, discogs_artist_id) = 1
                   AND (musicbrainz_artist_id IS NULL) <> (discogs_artist_id IS NULL)
                   AS various
        FROM credits
    ),
    rule_probe AS MATERIALIZED (
        SELECT DISTINCT
               CASE look.kind WHEN 'own' THEN c.mb WHEN 'alone' THEN c.mb ELSE bae_various_artists_id('musicbrainz') END AS mb,
               CASE look.kind WHEN 'own' THEN c.dg WHEN 'various' THEN bae_various_artists_id('discogs') END AS dg
        FROM rule_credit AS c
        JOIN (SELECT 'own' AS kind UNION ALL SELECT 'alone' UNION ALL SELECT 'various'
              UNION ALL SELECT 'various alone') AS look
            ON look.kind = 'own' OR (look.kind = 'alone' AND c.mb IS NOT NULL)
               OR (look.kind IN ('various', 'various alone') AND c.various)
    ),
    rule_probe_id AS MATERIALIZED (
        SELECT DISTINCT side.col, CASE side.col WHEN 'mb' THEN p.mb ELSE p.dg END AS cid,
               CASE side.col WHEN 'mb' THEN bae_artist_identity(p.mb, NULL)
                    ELSE bae_artist_identity(NULL, p.dg) END AS derived
        FROM rule_probe AS p
        JOIN (SELECT 'mb' AS col UNION ALL SELECT 'dg') AS side
            ON CASE side.col WHEN 'mb' THEN p.mb ELSE p.dg END IS NOT NULL
    ),
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
        JOIN artists AS a ON a.id = (SELECT shown.shown_artist_id FROM @crate::records::spread_fragments::ShownAs AS shown WHERE shown.artist_id = named_id)
    ),
    rule_held AS MATERIALIZED (
        SELECT col, cid, MIN(id) AS id,
               json_group_array(name ORDER BY id) ->> 0 AS name,
               json_group_array(sort_name ORDER BY id) FILTER (WHERE sort_name IS NOT NULL)
                   ->> 0 AS sort_name,
               json_group_array(mb ORDER BY id) FILTER (WHERE mb IS NOT NULL) ->> 0 AS mb,
               json_group_array(dg ORDER BY id) FILTER (WHERE dg IS NOT NULL) ->> 0 AS dg
        FROM rule_holding GROUP BY col, cid
    ),
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
                     CASE WHEN x.by_va THEN bae_various_artists_id('musicbrainz') ELSE x.mb END AS step_mb,
                     CASE WHEN x.by_va THEN bae_various_artists_id('discogs') ELSE x.dg END AS step_dg,
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
                        ON c.various AND various.mb = bae_various_artists_id('musicbrainz') AND various.dg = bae_various_artists_id('discogs')) AS x)
             AS step
        LEFT JOIN rule_match AS alone
            ON step.kind = 'disagrees' AND alone.mb IS step.step_mb AND alone.dg IS NULL
    ),
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
                   UNION SELECT credit_name.name_key, (SELECT shown.shown_artist_id FROM @crate::records::spread_fragments::ShownAs AS shown WHERE shown.artist_id = credit_name.artist_id)
                   FROM artist_credits AS credit_name) AS named
            ON r.id IS NULL AND r.name_key <> '' AND named.name_key = r.name_key
        LEFT JOIN artists AS a
            ON a.id = named.artist_id AND EXISTS (SELECT 1 FROM @crate::records::spread_fragments::ShownAs AS shown WHERE shown.artist_id = a.id AND shown.shown_artist_id = a.id)
           AND (a.musicbrainz_artist_id IS NULL OR r.name_mb IS NULL
                OR a.musicbrainz_artist_id = r.name_mb)
           AND (a.discogs_artist_id IS NULL OR r.name_dg IS NULL OR a.discogs_artist_id = r.name_dg)
        WHERE r.id IS NOT NULL OR a.id IS NOT NULL
    )
    SELECT c.name_key, c.musicbrainz_artist_id, c.discogs_artist_id,
           COALESCE((SELECT json_group_array(json_object(
                       'artist_id', a.artist_id, 'name', a.name, 'sort_name', a.sort_name,
                       'musicbrainz_artist_id', a.artist_musicbrainz_id,
                       'discogs_artist_id', a.artist_discogs_id) ORDER BY a.position)
            FROM credit_artist AS a
            WHERE a.name_key = c.name_key
              AND a.musicbrainz_artist_id IS c.musicbrainz_artist_id
              AND a.discogs_artist_id IS c.discogs_artist_id), '[]') AS artists
    FROM (SELECT DISTINCT name_key, musicbrainz_artist_id, discogs_artist_id FROM credits) AS c
"#);

impl CreditAnswers {
    /// The credit's folded name and catalog ids, and the JSON array of the
    /// artists the rule names for it, in the rule's order.
    pub fn into_columns(self) -> (Option<String>, Option<String>, Option<String>, String) {
        (self.name_key, self.musicbrainz_artist_id, self.discogs_artist_id, self.artists)
    }
}
