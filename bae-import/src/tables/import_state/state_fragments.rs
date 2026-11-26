//! The current state of a candidate or of one identify run, as the candidate pane and the runtime read it.

sqlive_compose::fragment!(
    CurrentState with owner(content_hash TEXT, identify_run INTEGER) {
        owner_row: String,
        provenance_row: Option<String>,
        provenance_kind: Option<String>,
        provenance_source: Option<String>,
        provenance_release_id: Option<String>,
        link_row: Option<String>,
        link_kind: Option<String>,
        link_source: Option<String>,
        link_release_id: Option<String>,
        link_partners: String,
        link_albums: String,
        choices_row: Option<String>,
        choices_disc_id_excluded: Option<bool>,
        choices_search_album: Option<String>,
        choices_search_artist: Option<String>,
        choices_chosen_catalogs: String,
        choices_discounted_catalogs: String,
        choices_excluded_barcodes: String,
        verdict_row: Option<String>,
        verdict_kind: Option<String>,
        verdict_track_count: Option<i64>,
        verdict_failures: String,
        verdict_error_detail: Option<String>,
        verdict_medium_conflict: Option<String>,
        verdict_kept_own_draft: Option<bool>,
        ledger_disc_id_step: Option<String>,
        ledger_barcode_step: Option<String>,
        ledger_catalog_step: Option<String>,
        ledger_isrc_step: Option<String>,
        ledger_search_step: Option<String>,
        ledger_search_album: Option<String>,
        ledger_search_artist: Option<String>,
        ledger_stray_unlisted: Option<i64>,
        ledger_providers: String,
        ledger_isrcs: String,
        ledger_isrc_shown: Option<String>,
        ledger_isrc_more_count: u32,
        ledger_offered_catalogs: String,
        ledger_offered_catalog_count: u32,
        ledger_rows: String,
        candidate_matches: String
    },
    r#"
    WITH statement(owner_row, provenance_row, provenance_kind, provenance_source, provenance_release_id, link_row, link_kind, link_source, link_release_id, link_partners, link_albums, choices_row, choices_disc_id_excluded, choices_search_album, choices_search_artist, choices_chosen_catalogs, choices_discounted_catalogs, choices_excluded_barcodes, verdict_row, verdict_kind, verdict_track_count, verdict_failures, verdict_error_detail, verdict_medium_conflict, verdict_kept_own_draft, ledger_disc_id_step, ledger_barcode_step, ledger_catalog_step, ledger_isrc_step, ledger_search_step, ledger_search_album, ledger_search_artist, ledger_stray_unlisted, ledger_providers, ledger_isrcs, ledger_isrc_shown, ledger_isrc_more_count, ledger_offered_catalogs, ledger_offered_catalog_count, ledger_rows, candidate_matches) AS (
            SELECT COALESCE(owner.content_hash, 'run ' || owner.identify_run) AS owner_row,
                  provenance.content_hash AS provenance_row, provenance.kind
                AS provenance_kind, provenance.source AS provenance_source,
                provenance.release_id AS provenance_release_id, link.content_hash AS link_row,
                link.kind AS link_kind, link.source AS link_source, link.release_id AS
                link_release_id,(
            SELECT json_group_array(json_array(partner.source, partner.release_id) ORDER BY
                partner.source)
            FROM import_candidate_release_link_partner AS partner
            WHERE partner.content_hash = link.content_hash
            ) AS link_partners,(
            SELECT json_group_array(json_array(album.catalog, album.album_id) ORDER BY
                bae_catalog_rank(album.catalog))
            FROM import_candidate_release_link_album AS album
            WHERE album.content_hash = link.content_hash
            ) AS link_albums, choices.content_hash AS choices_row, choices.disc_id_excluded AS
                choices_disc_id_excluded, choices.search_album AS choices_search_album,
                choices.search_artist AS choices_search_artist,(
            SELECT json_group_array(chosen.value ORDER BY chosen.position)
            FROM import_candidate_chosen_catalog AS chosen
            WHERE chosen.content_hash = choices.content_hash
            ) AS choices_chosen_catalogs,(
            SELECT json_group_array(struck.value ORDER BY struck.value)
            FROM import_candidate_discounted_catalog AS struck
            WHERE struck.content_hash = choices.content_hash
            ) AS choices_discounted_catalogs,(
            SELECT json_group_array(left_out.value ORDER BY left_out.value)
            FROM import_candidate_excluded_barcode AS left_out
            WHERE left_out.content_hash = choices.content_hash
            ) AS choices_excluded_barcodes, COALESCE(verdict.content_hash, 'run ' ||
                verdict.identify_run) AS verdict_row, verdict.kind AS verdict_kind,
                verdict.track_count AS verdict_track_count,(
            SELECT json_group_array(json_object('lookup', failure.lookup, 'catalog',
                failure.catalog, 'failure', failure.failure, 'status', failure.failure_status)
                ORDER BY failure.position)
            FROM import_candidate_verdict_failure AS failure
            WHERE failure.verdict_id = verdict.verdict_id AND (TRUE)
            ) AS verdict_failures, verdict.error_detail AS verdict_error_detail,
                verdict.medium_conflict AS verdict_medium_conflict, verdict.kept_own_draft AS
                verdict_kept_own_draft, verdict.disc_id_step AS ledger_disc_id_step,
                verdict.barcode_step AS ledger_barcode_step, verdict.catalog_step AS
                ledger_catalog_step, verdict.isrc_step AS ledger_isrc_step, verdict.search_step
                AS ledger_search_step, verdict.search_album AS ledger_search_album,
                verdict.search_artist AS ledger_search_artist,(
            SELECT MIN(unlisted.position)
            FROM import_candidate_match AS unlisted
                WHERE unlisted.verdict_id = verdict.verdict_id AND unlisted.listing = 'unlisted'
                    AND NOT EXISTS (
                SELECT 1
                FROM import_candidate_lookup_release AS found
                WHERE found.verdict_id = verdict.verdict_id AND found.match_position =
                    unlisted.position
            )
            ) AS ledger_stray_unlisted,(
            SELECT json_group_array(entry.catalog ORDER BY entry.position)
            FROM import_candidate_lookup_provider AS entry
            WHERE entry.verdict_id = verdict.verdict_id
            ) AS ledger_providers,(
            SELECT json_group_array(entry.isrc ORDER BY entry.position)
            FROM import_candidate_lookup_isrc AS entry
            WHERE entry.verdict_id = verdict.verdict_id
            ) AS ledger_isrcs,(
            SELECT entry.isrc
            FROM import_candidate_lookup_isrc AS entry
            WHERE entry.verdict_id = verdict.verdict_id
            ORDER BY entry.position
            LIMIT 1
            ) AS ledger_isrc_shown,(
            SELECT MAX(COUNT(*) - 1, 0)
            FROM import_candidate_lookup_isrc AS entry
            WHERE entry.verdict_id = verdict.verdict_id
            ) AS ledger_isrc_more_count,(
            SELECT json_group_array(entry.value ORDER BY entry.position)
            FROM import_candidate_lookup_offered_catalog AS entry
            WHERE entry.verdict_id = verdict.verdict_id
            ) AS ledger_offered_catalogs,(
            SELECT COUNT(*)
            FROM import_candidate_lookup_offered_catalog AS entry
            WHERE entry.verdict_id = verdict.verdict_id
            ) AS ledger_offered_catalog_count,(
            SELECT json_group_object(by_step.step, json(by_step.rows))
                FROM (
                    SELECT ledger_row.step AS step, json_group_array(json_object('step',
                        ledger_row.step, 'value', ledger_row.value, 'excluded',
                        ledger_row.excluded, 'cells', json((
                        SELECT json_group_array(json_object('catalog', lookup.catalog, 'answer',
                            CASE WHEN lookup.answer = 'answered' AND NOT EXISTS (
                        SELECT 1
                        FROM import_candidate_lookup_release AS found
                        WHERE found.verdict_id = lookup.verdict_id AND found.step = lookup.step
                            AND found.position = lookup.position AND found.cell = lookup.cell
                        ) THEN 'no_match' ELSE lookup.answer END, 'reason',
                            lookup.not_asked_reason, 'failure', lookup.failure, 'status',
                            lookup.failure_status, 'count',(
                        SELECT COUNT(DISTINCT found.pressing)
                        FROM import_candidate_lookup_release AS found
                        WHERE found.verdict_id = lookup.verdict_id AND found.step = lookup.step
                            AND found.position = lookup.position AND found.cell = lookup.cell
                        ), 'cards', json((
                            WITH recs AS (
                            SELECT m.*
                                FROM (
                                    SELECT found.ordinal AS ord, found.card, found.pressing,
                                        found.record_order, found.section, found.row_order,
                                        'answered' AS kind, matched.source, matched.release_id,
                                        0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, 0 AS
                                        a_label, 0 AS a_year, 0 AS a_country, 0 AS a_title, 0 AS
                                        a_artist, NULL AS named_note,(EXISTS(
                                    SELECT 1
                                    FROM import_candidate_release_link AS release_link
                                        WHERE release_link.content_hash =
                                            COALESCE(verdict.content_hash,(
                                        SELECT run_standing.content_hash
                                        FROM import_candidate_standing AS run_standing
                                        WHERE run_standing.identify_run = verdict.identify_run
                                    )) AND release_link.kind = 'pressing' AND
                                        release_link.source = matched.source AND
                                        release_link.release_id = matched.release_id
                                    ) OR (0 AND NOT EXISTS (
                                    SELECT 1
                                    FROM import_candidate_release_link AS release_link
                                        WHERE release_link.content_hash =
                                            COALESCE(verdict.content_hash,(
                                        SELECT run_standing.content_hash
                                        FROM import_candidate_standing AS run_standing
                                        WHERE run_standing.identify_run = verdict.identify_run
                                    )) AND release_link.kind = 'pressing'
                                ))) AS chosen, 0 AS pick_being_read
                                FROM import_candidate_lookup_release AS found
                                JOIN import_candidate_match AS matched ON matched.verdict_id =
                                    found.verdict_id AND matched.position = found.match_position
                                    WHERE found.verdict_id =(
                                    SELECT of_verdict.verdict_id
                                    FROM import_candidate_verdict AS of_verdict
                                    WHERE of_verdict.verdict_id = lookup.verdict_id
                                ) AND found.step = lookup.step AND found.position =
                                    lookup.position AND found.cell = lookup.cell
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
                                FROM @bae_records::records::spread_fragments::HeldPressing AS
                                    held
                                WHERE held.catalog = r.source AND held.key = r.release_id
                            )) FILTER (WHERE r.record_order = 0) AS in_library,
                                json_object('disc_id', MAX(r.a_disc_id), 'barcode',
                                MAX(r.a_barcode), 'catalog', MAX(r.a_catalog), 'label',
                                MAX(r.a_label), 'year', MAX(r.a_year), 'country',
                                MAX(r.a_country), 'title', MAX(r.a_title), 'artist',
                                MAX(r.a_artist)) AS agreed
                            FROM recs AS r
                                JOIN (
                                    SELECT found.ordinal AS ord, json(@crate::tables::import_state::match_record::MatchRecord(parent: matched)) AS record
                                FROM import_candidate_lookup_release AS found
                                JOIN import_candidate_match AS matched ON matched.verdict_id =
                                    found.verdict_id AND matched.position = found.match_position
                                    WHERE found.verdict_id =(
                                    SELECT of_verdict.verdict_id
                                    FROM import_candidate_verdict AS of_verdict
                                    WHERE of_verdict.verdict_id = lookup.verdict_id
                                ) AND found.step = lookup.step AND found.position =
                                    lookup.position AND found.cell = lookup.cell
                            ) AS rc ON rc.ord = r.ord
                            GROUP BY r.card, r.pressing, r.kind
                            ), notes AS (
                            SELECT g.card, g.pressing, g.kind,
                                json_group_array(json_object('note', g.note, 'catalogs',
                                json(CASE WHEN g.noted > 1 THEN g.catalogs ELSE '[]' END)) ORDER
                                BY g.first) AS notes
                                FROM (
                                SELECT h.*, COUNT(*) OVER (PARTITION BY h.card, h.pressing,
                                    h.kind) AS noted
                                    FROM (
                                    SELECT d.card, d.pressing, d.kind, d.named_note AS note,
                                        MIN(d.first) AS first, json_group_array(d.source ORDER
                                        BY bae_catalog_rank(d.source)) AS catalogs
                                        FROM (
                                        SELECT DISTINCT r.card, r.pressing, r.kind,
                                            r.named_note, r.source, MIN(r.record_order) OVER
                                            (PARTITION BY r.card, r.pressing, r.kind,
                                            r.named_note) AS first
                                        FROM recs AS r
                                        WHERE r.named_note IS NOT NULL
                                    ) AS d
                                    GROUP BY d.card, d.pressing, d.kind, d.named_note
                                ) AS h
                            ) AS g
                            GROUP BY g.card, g.pressing, g.kind
                            ), rowj AS (
                                SELECT p.*, json_object('records', json(p.records),
                                    'in_library', p.in_library, 'agreed', json(p.agreed),
                                    'chosen', p.chosen, 'pick_being_read', p.pick_being_read,
                                    'notes', json(COALESCE(n.notes, '[]')), 'statements', json((
                                SELECT json_group_array(json_object('fact', t.fact, 'line',
                                    t.line, 'ordinal', t.ordinal, 'value', t.value, 'catalogs',
                                    json(t.catalogs)) ORDER BY t.line, t.ordinal)
                                FROM import_candidate_row_statement AS t
                                    WHERE t.list =(
                                    SELECT l.list
                                    FROM import_candidate_card_list AS l
                                    WHERE l.verdict_id = lookup.verdict_id AND l.step =
                                        lookup.step AND l.position = lookup.position AND l.cell
                                        = lookup.cell
                                ) AND t.listing = p.kind AND t.pressing = p.pressing
                            ))) AS row
                            FROM pressings AS p
                            LEFT JOIN notes AS n ON n.card = p.card AND n.pressing = p.pressing
                                AND n.kind = p.kind
                            ), secj AS (
                            SELECT r.card, r.section, json_object('pressings',
                                json((json_group_array(json(r.row) ORDER BY r.row_order) FILTER
                                (WHERE r.kind <> 'narrowed_out'))), 'narrowed_out',
                                json((json_group_array(json(r.row) ORDER BY r.row_order) FILTER
                                (WHERE r.kind = 'narrowed_out')))) AS section_json,
                                json_group_array(json(r.lead) ORDER BY r.row_order) FILTER
                                (WHERE r.kind <> 'narrowed_out') AS leads
                            FROM rowj AS r
                            GROUP BY r.card, r.section
                            ), card_secs AS (
                            SELECT s.card, json_group_array(json(s.section_json) ORDER BY
                                s.section) AS sections, json_group_array(json(s.leads) ORDER BY
                                s.section) AS leads
                            FROM secj AS s
                            GROUP BY s.card
                            ) SELECT json_group_array(json_object('id', c.id, 'titles', json((
                            SELECT json_group_array(t.text ORDER BY t.ordinal)
                            FROM import_candidate_card_text AS t
                                WHERE t.list =(
                                SELECT l.list
                                FROM import_candidate_card_list AS l
                                WHERE l.verdict_id = lookup.verdict_id AND l.step = lookup.step
                                    AND l.position = lookup.position AND l.cell = lookup.cell
                            ) AND t.card = c.card AND t.kind = 'title'
                            )), 'artists', json((
                            SELECT json_group_array(t.text ORDER BY t.ordinal)
                            FROM import_candidate_card_text AS t
                                WHERE t.list =(
                                SELECT l.list
                                FROM import_candidate_card_list AS l
                                WHERE l.verdict_id = lookup.verdict_id AND l.step = lookup.step
                                    AND l.position = lookup.position AND l.cell = lookup.cell
                            ) AND t.card = c.card AND t.kind = 'artist'
                            )), 'labels', json((
                            SELECT json_group_array(t.text ORDER BY t.ordinal)
                            FROM import_candidate_card_text AS t
                                WHERE t.list =(
                                SELECT l.list
                                FROM import_candidate_card_list AS l
                                WHERE l.verdict_id = lookup.verdict_id AND l.step = lookup.step
                                    AND l.position = lookup.position AND l.cell = lookup.cell
                            ) AND t.card = c.card AND t.kind = 'label'
                            )), 'cover', json((
                            SELECT json(rc.record)
                                FROM (
                                    SELECT found.ordinal AS ord, json(@crate::tables::import_state::match_record::MatchRecord(parent: matched)) AS record
                                FROM import_candidate_lookup_release AS found
                                JOIN import_candidate_match AS matched ON matched.verdict_id =
                                    found.verdict_id AND matched.position = found.match_position
                                    WHERE found.verdict_id =(
                                    SELECT of_verdict.verdict_id
                                    FROM import_candidate_verdict AS of_verdict
                                    WHERE of_verdict.verdict_id = lookup.verdict_id
                                ) AND found.step = lookup.step AND found.position =
                                    lookup.position AND found.cell = lookup.cell
                            ) AS rc
                            WHERE rc.ord = c.cover_ord
                            )), 'leads', json((
                            SELECT json_group_array(json(lead.value))
                            FROM json_each(cs.leads) AS sec, json_each(sec.value) AS lead
                        )), 'sources', json(c.sources), 'headings', json(c.headings),
                            'year_min', c.year_min, 'year_max', c.year_max, 'sections',
                            json(cs.sections)) ORDER BY c.card_order)
                            FROM (
                            SELECT t.*
                            FROM import_candidate_card AS t
                                WHERE t.list =(
                                SELECT l.list
                                FROM import_candidate_card_list AS l
                                WHERE l.verdict_id = lookup.verdict_id AND l.step = lookup.step
                                    AND l.position = lookup.position AND l.cell = lookup.cell
                            )
                        ) AS c
                        JOIN card_secs AS cs ON cs.card = c.card
                    ))) ORDER BY lookup.cell)
                    FROM import_candidate_lookup AS lookup
                    WHERE lookup.verdict_id = ledger_row.verdict_id AND lookup.step =
                        ledger_row.step AND lookup.position = ledger_row.position
                ))) ORDER BY ledger_row.position) AS rows
                FROM import_candidate_lookup_value AS ledger_row
                WHERE ledger_row.verdict_id = verdict.verdict_id
                GROUP BY ledger_row.step
            ) AS by_step
            ) AS ledger_rows,(
                SELECT json_group_array(json(@crate::tables::import_state::match_record::MatchRecord(parent: found)) ORDER BY found.position)
            FROM import_candidate_match AS found
            WHERE found.verdict_id = verdict.verdict_id
        ) AS candidate_matches
        FROM owner
                LEFT JOIN import_candidate_draft_provenance AS provenance ON provenance.content_hash =
            owner.content_hash
        LEFT JOIN import_candidate_release_link AS link ON link.content_hash =
            owner.content_hash
        LEFT JOIN import_candidate_lookup_choices AS choices ON choices.content_hash =
            owner.content_hash
        LEFT JOIN import_candidate_verdict AS verdict ON verdict.content_hash IS
            owner.content_hash AND verdict.identify_run IS owner.identify_run
    )
    SELECT statement.owner_row, statement.provenance_row, statement.provenance_kind, statement.provenance_source, statement.provenance_release_id, statement.link_row, statement.link_kind, statement.link_source, statement.link_release_id, statement.link_partners, statement.link_albums, statement.choices_row, statement.choices_disc_id_excluded, statement.choices_search_album, statement.choices_search_artist, statement.choices_chosen_catalogs, statement.choices_discounted_catalogs, statement.choices_excluded_barcodes, statement.verdict_row, statement.verdict_kind, statement.verdict_track_count, statement.verdict_failures, statement.verdict_error_detail, statement.verdict_medium_conflict, statement.verdict_kept_own_draft, statement.ledger_disc_id_step, statement.ledger_barcode_step, statement.ledger_catalog_step, statement.ledger_isrc_step, statement.ledger_search_step, statement.ledger_search_album, statement.ledger_search_artist, statement.ledger_stray_unlisted, statement.ledger_providers, statement.ledger_isrcs, statement.ledger_isrc_shown, statement.ledger_isrc_more_count, statement.ledger_offered_catalogs, statement.ledger_offered_catalog_count, statement.ledger_rows, statement.candidate_matches, signals.*, state_row.*
    FROM statement
    LEFT JOIN @crate::tables::import_state::state_fragments::StateRow(owner: (SELECT * FROM owner))
        AS state_row ON state_row.owner_row = statement.owner_row
    LEFT JOIN @crate::tables::import_state::state_fragments::Signals(owner: (SELECT * FROM owner))
        AS signals ON signals.owner_row = statement.owner_row
"#);

sqlive_compose::fragment!(
    SignalTextPool with owner(content_hash TEXT, identify_run INTEGER) {
        owner_row: String,
        signals_text_pool: String
    },
    r#"
    SELECT COALESCE(owner.content_hash, 'run ' || owner.identify_run) AS owner_row, COALESCE((
            SELECT json_group_array(json_object('text', line.text, 'origin', line.origin) ORDER BY
                line.position)
            FROM import_candidate_signals AS signals
            JOIN import_candidate_text_line AS line ON line.signals_id = signals.signals_id
            WHERE signals.content_hash IS owner.content_hash AND signals.identify_run IS
                owner.identify_run
        ), '[]') AS signals_text_pool
    FROM owner
"#);

sqlive_compose::fragment!(
    Signals with owner(content_hash TEXT, identify_run INTEGER) {
        owner_row: String,
        signals_not_cd_rate: Option<u32>,
        signals_discs: String,
        signals_barcodes: String,
        signals_catalogs: String,
        signals_free_text: String,
        signals_isrcs: String,
        signals_track_titles: String
    },
    r#"
    WITH statement(owner_row, signals_audio_source, signals_cd_rip_proof, signals_download_proof, signals_audio_source_file, signals_not_cd_rate, signals_disc_id_state, signals_disc_id_failure, signals_barcode_state, signals_barcode_failure, signals_text_state, signals_text_failure, signals_discs, signals_barcodes, signals_catalogs, signals_free_text, signals_isrcs, signals_track_titles) AS (
        SELECT COALESCE(owner.content_hash, 'run ' || owner.identify_run) AS owner_row,
            signals.audio_source AS
                signals_audio_source, signals.cd_rip_proof AS signals_cd_rip_proof,
                signals.download_proof AS signals_download_proof, signals.audio_source_file AS
                signals_audio_source_file, signals.not_cd_rate AS signals_not_cd_rate,
                signals.disc_id_state AS signals_disc_id_state, signals.disc_id_failure AS
                signals_disc_id_failure, signals.barcode_state AS signals_barcode_state,
                signals.barcode_failure AS signals_barcode_failure, signals.text_state AS
                signals_text_state, signals.text_failure AS signals_text_failure,(
            SELECT json_group_array(json_object('value', listed.value, 'origin_path',
                listed.origin_path) ORDER BY listed.position)
            FROM import_candidate_signal_value AS listed
            WHERE listed.signals_id = signals.signals_id AND listed.list = 'disc_id'
            ) AS signals_discs,(
            SELECT json_group_array(json_object('value', listed.value, 'origin_path',
                listed.origin_path) ORDER BY listed.position)
            FROM import_candidate_signal_value AS listed
            WHERE listed.signals_id = signals.signals_id AND listed.list = 'barcode'
            ) AS signals_barcodes,(
            SELECT json_group_array(listed.value ORDER BY listed.position)
            FROM import_candidate_signal_value AS listed
            WHERE listed.signals_id = signals.signals_id AND listed.list = 'catalog'
            ) AS signals_catalogs,(
            SELECT json_group_array(listed.value ORDER BY listed.position)
            FROM import_candidate_signal_value AS listed
            WHERE listed.signals_id = signals.signals_id AND listed.list = 'free_text'
            ) AS signals_free_text,(
            SELECT json_group_array(listed.value ORDER BY listed.position)
            FROM import_candidate_signal_value AS listed
            WHERE listed.signals_id = signals.signals_id AND listed.list = 'isrc'
            ) AS signals_isrcs,(
            SELECT json_group_array(listed.value ORDER BY listed.position)
            FROM import_candidate_signal_value AS listed
            WHERE listed.signals_id = signals.signals_id AND listed.list = 'track_title'
            ) AS signals_track_titles
        FROM owner
        JOIN import_candidate_signals AS signals ON signals.content_hash IS owner.content_hash AND
            signals.identify_run IS owner.identify_run
    )
    SELECT * FROM statement
"#);

sqlive_compose::fragment!(
    StateRow with owner(content_hash TEXT, identify_run INTEGER) {
        owner_row: String,
        candidate_key: crate::CandidateKey,
        draft_author: Option<String>,
        edits_revision: i64,
        edits_sheet_places: String,
        edits_sheet_references: String
    },
    r#"
    SELECT COALESCE(owner.content_hash, 'run ' || owner.identify_run) AS owner_row,
        state.content_hash, state.candidate_key, state.metadata_revision, edit.author AS draft_author, state.edit_revision AS edits_revision, COALESCE((
            SELECT json_group_array(json_object('sheet_id', place.sheet_id, 'place',
                place.place, 'medium', place.medium, 'side', place.side) ORDER BY
                place.sheet_id)
            FROM import_candidate_sheet_place AS place
            WHERE place.content_hash = state.content_hash
            ), '[]') AS edits_sheet_places, COALESCE((
            SELECT json_group_array(json_object('sheet_id', binding.sheet_id, 'file_reference',
                binding.file_reference, 'file_id', binding.file_id) ORDER BY binding.sheet_id,
                binding.file_reference)
            FROM import_candidate_sheet_reference AS binding
            WHERE binding.content_hash = state.content_hash
            ), '[]') AS edits_sheet_references
    FROM owner
    JOIN import_candidate_state AS state ON state.content_hash = owner.content_hash
    LEFT JOIN import_candidate_edit AS edit ON edit.content_hash = state.content_hash
"#);
