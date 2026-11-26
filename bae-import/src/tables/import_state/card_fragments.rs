//! The cards of a verdict, as the candidate pane and the runtime read them.

sqlive_compose::fragment!(
    VerdictCards with owner(content_hash TEXT, identify_run INTEGER) {
        owner_row: String,
        verdict_cards: Option<String>
    },
    r#"
    SELECT COALESCE(owner.content_hash, 'run ' || owner.identify_run) AS owner_row, (
            SELECT (
                WITH listed_cards AS MATERIALIZED (
                    SELECT (SELECT cards.cards FROM @crate::cards::Cards(members: (
                                SELECT found.position AS ord, found.card, found.pressing,
                                    found.record_order, found.section, found.row_order,
                                    found.listing AS kind, found.source, found.release_id,
                                    found.agrees_disc_id AS a_disc_id, found.agrees_barcode AS
                                    a_barcode, found.agrees_catalog AS a_catalog,
                                    found.agrees_label AS a_label, found.agrees_year AS a_year,
                                    found.agrees_country AS a_country, found.agrees_title AS
                                    a_title, found.agrees_artist AS a_artist,
                                    found.named_note,(EXISTS(
                                SELECT 1
                                FROM import_candidate_release_link AS release_link
                                    WHERE release_link.content_hash =
                                        COALESCE(verdict.content_hash,(
                                    SELECT run_standing.content_hash
                                    FROM import_candidate_standing AS run_standing
                                    WHERE run_standing.identify_run = verdict.identify_run
                                )) AND release_link.kind = 'pressing' AND release_link.source =
                                    found.source AND release_link.release_id = found.release_id
                                ) OR ((EXISTS(
                                SELECT 1
                                FROM import_candidate_standing AS reading
                                    WHERE reading.content_hash = COALESCE(verdict.content_hash,(
                                    SELECT run_standing.content_hash
                                    FROM import_candidate_standing AS run_standing
                                    WHERE run_standing.identify_run = verdict.identify_run
                                )) AND reading.identification = 'reading_pick'
                                ) AND found.release_id IS verdict.unattended_pick) AND NOT
                                    EXISTS (
                                SELECT 1
                                FROM import_candidate_release_link AS release_link
                                    WHERE release_link.content_hash =
                                        COALESCE(verdict.content_hash,(
                                    SELECT run_standing.content_hash
                                    FROM import_candidate_standing AS run_standing
                                    WHERE run_standing.identify_run = verdict.identify_run
                                )) AND release_link.kind = 'pressing'
                                ))) AS chosen,(EXISTS(
                                SELECT 1
                                FROM import_candidate_standing AS reading
                                    WHERE reading.content_hash = COALESCE(verdict.content_hash,(
                                    SELECT run_standing.content_hash
                                    FROM import_candidate_standing AS run_standing
                                    WHERE run_standing.identify_run = verdict.identify_run
                                )) AND reading.identification = 'reading_pick'
                            ) AND found.release_id IS verdict.unattended_pick) AS
                                pick_being_read
                            FROM import_candidate_match AS found
                            WHERE found.verdict_id = verdict.verdict_id AND found.listing IN
                                ('offered', 'narrowed_out')
                        ), records: (
                                SELECT found.position AS ord, json(@crate::tables::import_state::match_record::MatchRecord(parent: found)) AS record
                            FROM import_candidate_match AS found
                            WHERE found.verdict_id = verdict.verdict_id AND found.listing IN
                                ('offered', 'narrowed_out')
                        ), card_list: (
                                SELECT l.list
                                FROM import_candidate_card_list AS l
                                WHERE l.verdict_id = verdict.verdict_id AND l.step IS NULL
                            )) AS cards) AS json
                ), offered AS (
                SELECT found.card, found.pressing, found.source, found.source_group_id
                FROM import_candidate_match AS found
                WHERE found.verdict_id = verdict.verdict_id AND found.listing = 'offered'
                ) SELECT json_object('cards', json(listed_cards.json), 'decision', json(CASE
                    WHEN verdict.kind = 'found' THEN json_object('unattended_pick',
                    verdict.unattended_pick, 'declined', verdict.declined, 'medium_conflict',
                    verdict.medium_conflict) END), 'failure_lines', json(CASE WHEN EXISTS (
                SELECT 1
                FROM import_candidate_verdict_failure AS failure
                    WHERE failure.verdict_id = verdict.verdict_id AND NOT EXISTS (
                    SELECT 1
                    FROM import_candidate_lookup AS cell
                    WHERE cell.verdict_id = failure.verdict_id AND cell.step = failure.lookup
                        AND cell.catalog IS failure.catalog AND cell.answer = 'failed' AND
                        cell.failure = failure.failure AND cell.failure_status IS
                        failure.failure_status
                )
                ) THEN json_object('all', json((
                SELECT json_group_array(json_object('lookup', failure.lookup, 'catalog',
                    failure.catalog, 'failure', failure.failure, 'status',
                    failure.failure_status) ORDER BY failure.position)
                FROM import_candidate_verdict_failure AS failure
                    WHERE failure.verdict_id = verdict.verdict_id AND (NOT EXISTS (
                    SELECT 1
                    FROM import_candidate_lookup AS cell
                    WHERE cell.verdict_id = failure.verdict_id AND cell.step = failure.lookup
                        AND cell.catalog IS failure.catalog AND cell.answer = 'failed' AND
                        cell.failure = failure.failure AND cell.failure_status IS
                        failure.failure_status
                ))
                )), 'pick_failures', json(NULLIF((
                SELECT json_group_array(json_object('lookup', failure.lookup, 'catalog',
                    failure.catalog, 'failure', failure.failure, 'status',
                    failure.failure_status) ORDER BY failure.position)
                FROM import_candidate_verdict_failure AS failure
                    WHERE failure.verdict_id = verdict.verdict_id AND (NOT EXISTS (
                    SELECT 1
                    FROM import_candidate_lookup AS cell
                    WHERE cell.verdict_id = failure.verdict_id AND cell.step = failure.lookup
                        AND cell.catalog IS failure.catalog AND cell.answer = 'failed' AND
                        cell.failure = failure.failure AND cell.failure_status IS
                        failure.failure_status
                ) AND failure.lookup = 'cover')
                ), '[]')), 'missing_searches', json((
                SELECT json_group_array(json_object('lookup', missing.lookup, 'catalog',
                    missing.catalog) ORDER BY missing.first)
                    FROM (
                    SELECT failure.lookup, COALESCE(failure.catalog, 'musicbrainz') AS catalog,
                        MIN(failure.position) AS first
                    FROM import_candidate_verdict_failure AS failure
                        WHERE failure.verdict_id = verdict.verdict_id AND NOT EXISTS (
                        SELECT 1
                        FROM import_candidate_lookup AS cell
                        WHERE cell.verdict_id = failure.verdict_id AND cell.step =
                            failure.lookup AND cell.catalog IS failure.catalog AND cell.answer =
                            'failed' AND cell.failure = failure.failure AND cell.failure_status
                            IS failure.failure_status
                    ) AND failure.lookup <> 'cover'
                    GROUP BY failure.lookup, COALESCE(failure.catalog, 'musicbrainz')
                ) AS missing
                ))) END), 'narrowed_out_count',(
                SELECT COUNT(DISTINCT found.pressing)
                FROM import_candidate_match AS found
                WHERE found.verdict_id = verdict.verdict_id AND found.listing = 'narrowed_out'
                ), 'shared_album', CASE WHEN (
                SELECT COUNT(DISTINCT card)
                FROM offered
                ) = 1 AND (
                SELECT COUNT(DISTINCT pressing)
                FROM offered
                ) >= 2 AND NOT EXISTS (
                SELECT 1
                FROM offered
                WHERE source_group_id IS NOT NULL
                GROUP BY source
                HAVING COUNT(DISTINCT source_group_id) > 1
                ) AND EXISTS (
                SELECT 1
                FROM offered
                WHERE source_group_id IS NOT NULL
                ) THEN json_object('leads', json((
                SELECT json_group_array(json(lead.value))
                FROM json_each(listed_cards.json) AS card, json_each(card.value, '$.leads') AS
                    lead
                )), 'albums', json((
                SELECT json_group_array(json_object('catalog', album.source, 'key',
                    album.group_id) ORDER BY bae_catalog_rank(album.source))
                    FROM (
                    SELECT source, MIN(source_group_id) AS group_id
                    FROM offered
                    WHERE source_group_id IS NOT NULL
                    GROUP BY source
                ) AS album
            ))) END)
            FROM listed_cards
        )
        FROM import_candidate_verdict AS verdict
            WHERE verdict.content_hash IS owner.content_hash AND verdict.identify_run IS
                owner.identify_run AND verdict.kind IN ('running', 'found', 'failed')
    ) AS verdict_cards
    FROM owner
"#);
