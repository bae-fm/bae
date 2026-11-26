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
                    SELECT (
                        WITH recs AS (
                        SELECT m.*
                            FROM (
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
                        ) AS m
                        ), pressings AS (
                            SELECT r.card, r.pressing, r.kind, MIN(r.section) AS section,
                                MIN(r.row_order) AS row_order, MAX(r.chosen) AS chosen,
                                MAX(r.pick_being_read) AS pick_being_read,
                                json_group_array(json(rc.record) ORDER BY r.record_order) AS
                                records, MAX(json_object('catalog', r.source, 'key',
                                r.release_id)) FILTER (WHERE r.record_order = 0) AS lead,
                                MAX(EXISTS(
                            SELECT 1
                            FROM @bae_records::records::spread_fragments::HeldPressing AS held
                            WHERE held.catalog = r.source AND held.key = r.release_id
                        )) FILTER (WHERE r.record_order = 0) AS in_library,
                            json_object('disc_id', MAX(r.a_disc_id), 'barcode',
                            MAX(r.a_barcode), 'catalog', MAX(r.a_catalog), 'label',
                            MAX(r.a_label), 'year', MAX(r.a_year), 'country', MAX(r.a_country),
                            'title', MAX(r.a_title), 'artist', MAX(r.a_artist)) AS agreed
                        FROM recs AS r
                            JOIN (
                                SELECT found.position AS ord, json_object('position',
                                    found.position, 'pressing', found.pressing, 'listing',
                                    found.listing, 'media_kind', found.media_kind,
                                    'album_links', found.album_links, 'source', found.source,
                                    'release_id', found.release_id, 'title', found.title,
                                    'credits', found.credits, 'year', found.year, 'country',
                                    found.country, 'region', found.region, 'status',
                                    found.status, 'packaging', found.packaging,
                                    'source_group_id', found.source_group_id, 'named_note',
                                    found.named_note, 'discogs_details',
                                    json(found.discogs_details), 'cover', CASE WHEN
                                    found.cover_url IS NOT NULL THEN json_object('url',
                                    found.cover_url, 'label', found.cover_label, 'source',
                                    found.cover_source, 'standing', found.cover_standing) END,
                                    'source_tracks', CASE WHEN found.source_tracks_kind =
                                    'listed' THEN json_object('kind', 'listed', 'count',
                                    found.source_tracks_count) WHEN found.source_tracks_kind IS
                                    NOT NULL THEN json_object('kind', found.source_tracks_kind)
                                    END, 'document_failure', CASE WHEN found.document_failure IS
                                    NOT NULL THEN json_object('kind', found.document_failure,
                                    'status', found.document_failure_status) END,
                                    'album_first_year',(
                                SELECT fetched.album_first_year
                                FROM source_release AS fetched
                                WHERE fetched.catalog = found.source AND fetched.release_id =
                                    found.release_id
                                ), 'labels', json((
                                SELECT json_group_array(json_object('name', entry.name,
                                    'catalog_number', entry.catalog_number,
                                    'musicbrainz_label_id', entry.musicbrainz_label_id,
                                    'discogs_label_id', entry.discogs_label_id) ORDER BY
                                    entry.ordinal)
                                FROM import_candidate_match_label AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'barcodes', json((
                                SELECT json_group_array(entry.barcode ORDER BY entry.ordinal)
                                FROM import_candidate_match_barcode AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'cover_copies', json((
                                SELECT json_group_array(json_object('url', copy.url, 'max_edge',
                                    copy.max_edge) ORDER BY copy.max_edge)
                                FROM import_candidate_match_cover_copy AS copy
                                WHERE copy.match_id = found.match_id
                                )), 'media', json((
                                SELECT json_group_array(json_object('kind', entry.media_kind,
                                    'medium', entry.medium, 'quantity', entry.quantity) ORDER BY
                                    entry.ordinal)
                                FROM import_candidate_match_medium AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'links', json((
                                SELECT json_group_array(json_array(entry.catalog, entry.key)
                                    ORDER BY entry.ordinal)
                                FROM import_candidate_match_link AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'album_link_rows', json((
                                SELECT json_group_array(json_object('catalog', entry.catalog,
                                    'key', entry.key) ORDER BY entry.ordinal)
                                FROM import_candidate_match_album_link AS entry
                                WHERE entry.match_id = found.match_id
                            ))) AS record
                            FROM import_candidate_match AS found
                            WHERE found.verdict_id = verdict.verdict_id AND found.listing IN
                                ('offered', 'narrowed_out')
                        ) AS rc ON rc.ord = r.ord
                        GROUP BY r.card, r.pressing, r.kind
                        ), notes AS (
                        SELECT g.card, g.pressing, g.kind, json_group_array(json_object('note',
                            g.note, 'catalogs', json(CASE WHEN g.noted > 1 THEN g.catalogs ELSE
                            '[]' END)) ORDER BY g.first) AS notes
                            FROM (
                            SELECT h.*, COUNT(*) OVER (PARTITION BY h.card, h.pressing, h.kind)
                                AS noted
                                FROM (
                                SELECT d.card, d.pressing, d.kind, d.named_note AS note,
                                    MIN(d.first) AS first, json_group_array(d.source ORDER BY
                                    bae_catalog_rank(d.source)) AS catalogs
                                    FROM (
                                    SELECT DISTINCT r.card, r.pressing, r.kind, r.named_note,
                                        r.source, MIN(r.record_order) OVER (PARTITION BY r.card,
                                        r.pressing, r.kind, r.named_note) AS first
                                    FROM recs AS r
                                    WHERE r.named_note IS NOT NULL
                                ) AS d
                                GROUP BY d.card, d.pressing, d.kind, d.named_note
                            ) AS h
                        ) AS g
                        GROUP BY g.card, g.pressing, g.kind
                        ), rowj AS (
                            SELECT p.*, json_object('records', json(p.records), 'in_library',
                                p.in_library, 'agreed', json(p.agreed), 'chosen', p.chosen,
                                'pick_being_read', p.pick_being_read, 'notes',
                                json(COALESCE(n.notes, '[]')), 'statements', json((
                            SELECT json_group_array(json_object('fact', t.fact, 'line', t.line,
                                'ordinal', t.ordinal, 'value', t.value, 'catalogs',
                                json(t.catalogs)) ORDER BY t.line, t.ordinal)
                            FROM import_candidate_row_statement AS t
                                WHERE t.list =(
                                SELECT l.list
                                FROM import_candidate_card_list AS l
                                WHERE l.verdict_id = verdict.verdict_id AND l.step IS NULL
                            ) AND t.listing = p.kind AND t.pressing = p.pressing
                        ))) AS row
                        FROM pressings AS p
                        LEFT JOIN notes AS n ON n.card = p.card AND n.pressing = p.pressing AND
                            n.kind = p.kind
                        ), secj AS (
                        SELECT r.card, r.section, json_object('pressings',
                            json((json_group_array(json(r.row) ORDER BY r.row_order) FILTER
                            (WHERE r.kind <> 'narrowed_out'))), 'narrowed_out',
                            json((json_group_array(json(r.row) ORDER BY r.row_order) FILTER
                            (WHERE r.kind = 'narrowed_out')))) AS section_json,
                            json_group_array(json(r.lead) ORDER BY r.row_order) FILTER (WHERE
                            r.kind <> 'narrowed_out') AS leads
                        FROM rowj AS r
                        GROUP BY r.card, r.section
                        ), card_secs AS (
                        SELECT s.card, json_group_array(json(s.section_json) ORDER BY s.section)
                            AS sections, json_group_array(json(s.leads) ORDER BY s.section) AS
                            leads
                        FROM secj AS s
                        GROUP BY s.card
                        ) SELECT json_group_array(json_object('id', c.id, 'titles', json((
                        SELECT json_group_array(t.text ORDER BY t.ordinal)
                        FROM import_candidate_card_text AS t
                            WHERE t.list =(
                            SELECT l.list
                            FROM import_candidate_card_list AS l
                            WHERE l.verdict_id = verdict.verdict_id AND l.step IS NULL
                        ) AND t.card = c.card AND t.kind = 'title'
                        )), 'artists', json((
                        SELECT json_group_array(t.text ORDER BY t.ordinal)
                        FROM import_candidate_card_text AS t
                            WHERE t.list =(
                            SELECT l.list
                            FROM import_candidate_card_list AS l
                            WHERE l.verdict_id = verdict.verdict_id AND l.step IS NULL
                        ) AND t.card = c.card AND t.kind = 'artist'
                        )), 'labels', json((
                        SELECT json_group_array(t.text ORDER BY t.ordinal)
                        FROM import_candidate_card_text AS t
                            WHERE t.list =(
                            SELECT l.list
                            FROM import_candidate_card_list AS l
                            WHERE l.verdict_id = verdict.verdict_id AND l.step IS NULL
                        ) AND t.card = c.card AND t.kind = 'label'
                        )), 'cover', json((
                        SELECT json(rc.record)
                            FROM (
                                SELECT found.position AS ord, json_object('position',
                                    found.position, 'pressing', found.pressing, 'listing',
                                    found.listing, 'media_kind', found.media_kind,
                                    'album_links', found.album_links, 'source', found.source,
                                    'release_id', found.release_id, 'title', found.title,
                                    'credits', found.credits, 'year', found.year, 'country',
                                    found.country, 'region', found.region, 'status',
                                    found.status, 'packaging', found.packaging,
                                    'source_group_id', found.source_group_id, 'named_note',
                                    found.named_note, 'discogs_details',
                                    json(found.discogs_details), 'cover', CASE WHEN
                                    found.cover_url IS NOT NULL THEN json_object('url',
                                    found.cover_url, 'label', found.cover_label, 'source',
                                    found.cover_source, 'standing', found.cover_standing) END,
                                    'source_tracks', CASE WHEN found.source_tracks_kind =
                                    'listed' THEN json_object('kind', 'listed', 'count',
                                    found.source_tracks_count) WHEN found.source_tracks_kind IS
                                    NOT NULL THEN json_object('kind', found.source_tracks_kind)
                                    END, 'document_failure', CASE WHEN found.document_failure IS
                                    NOT NULL THEN json_object('kind', found.document_failure,
                                    'status', found.document_failure_status) END,
                                    'album_first_year',(
                                SELECT fetched.album_first_year
                                FROM source_release AS fetched
                                WHERE fetched.catalog = found.source AND fetched.release_id =
                                    found.release_id
                                ), 'labels', json((
                                SELECT json_group_array(json_object('name', entry.name,
                                    'catalog_number', entry.catalog_number,
                                    'musicbrainz_label_id', entry.musicbrainz_label_id,
                                    'discogs_label_id', entry.discogs_label_id) ORDER BY
                                    entry.ordinal)
                                FROM import_candidate_match_label AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'barcodes', json((
                                SELECT json_group_array(entry.barcode ORDER BY entry.ordinal)
                                FROM import_candidate_match_barcode AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'cover_copies', json((
                                SELECT json_group_array(json_object('url', copy.url, 'max_edge',
                                    copy.max_edge) ORDER BY copy.max_edge)
                                FROM import_candidate_match_cover_copy AS copy
                                WHERE copy.match_id = found.match_id
                                )), 'media', json((
                                SELECT json_group_array(json_object('kind', entry.media_kind,
                                    'medium', entry.medium, 'quantity', entry.quantity) ORDER BY
                                    entry.ordinal)
                                FROM import_candidate_match_medium AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'links', json((
                                SELECT json_group_array(json_array(entry.catalog, entry.key)
                                    ORDER BY entry.ordinal)
                                FROM import_candidate_match_link AS entry
                                WHERE entry.match_id = found.match_id
                                )), 'album_link_rows', json((
                                SELECT json_group_array(json_object('catalog', entry.catalog,
                                    'key', entry.key) ORDER BY entry.ordinal)
                                FROM import_candidate_match_album_link AS entry
                                WHERE entry.match_id = found.match_id
                            ))) AS record
                            FROM import_candidate_match AS found
                            WHERE found.verdict_id = verdict.verdict_id AND found.listing IN
                                ('offered', 'narrowed_out')
                        ) AS rc
                        WHERE rc.ord = c.cover_ord
                        )), 'leads', json((
                        SELECT json_group_array(json(lead.value))
                        FROM json_each(cs.leads) AS sec, json_each(sec.value) AS lead
                    )), 'sources', json(c.sources), 'headings', json(c.headings), 'year_min',
                        c.year_min, 'year_max', c.year_max, 'sections', json(cs.sections)) ORDER
                        BY c.card_order)
                        FROM (
                        SELECT t.*
                        FROM import_candidate_card AS t
                            WHERE t.list =(
                            SELECT l.list
                            FROM import_candidate_card_list AS l
                            WHERE l.verdict_id = verdict.verdict_id AND l.step IS NULL
                        )
                    ) AS c
                    JOIN card_secs AS cs ON cs.card = c.card
                ) AS json
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
