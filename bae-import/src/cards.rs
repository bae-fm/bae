//! The cards of one list of releases, as the verdict's cards, the typed search
//! and the ledger's cells show them. Each statement that spreads the fragment
//! hands it the list's releases, their records and the list's id.

sqlive_compose::fragment!(
    Cards with
        members(ord INTEGER, card INTEGER, pressing INTEGER, record_order INTEGER,
            section INTEGER, row_order INTEGER, kind TEXT, source TEXT, release_id TEXT,
            a_disc_id INTEGER, a_barcode INTEGER, a_catalog INTEGER, a_label INTEGER,
            a_year INTEGER, a_country INTEGER, a_title INTEGER, a_artist INTEGER,
            named_note TEXT, chosen INTEGER, pick_being_read INTEGER),
        records(ord INTEGER, record TEXT),
        card_list(list INTEGER)
    {
        cards: Option<String>
    },
    r#"
    SELECT (
                    WITH recs AS (
                    SELECT m.*
                        FROM members AS m
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
                        JOIN records AS rc ON rc.ord = r.ord
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
                            WHERE t.list =(SELECT card_list.list FROM card_list) AND t.listing = p.kind AND t.pressing = p.pressing
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
                        WHERE t.list =(SELECT card_list.list FROM card_list) AND t.card = c.card AND t.kind = 'title'
                    )), 'artists', json((
                    SELECT json_group_array(t.text ORDER BY t.ordinal)
                    FROM import_candidate_card_text AS t
                        WHERE t.list =(SELECT card_list.list FROM card_list) AND t.card = c.card AND t.kind = 'artist'
                    )), 'labels', json((
                    SELECT json_group_array(t.text ORDER BY t.ordinal)
                    FROM import_candidate_card_text AS t
                        WHERE t.list =(SELECT card_list.list FROM card_list) AND t.card = c.card AND t.kind = 'label'
                    )), 'cover', json((
                    SELECT json(rc.record)
                        FROM records AS rc
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
                        WHERE t.list =(SELECT card_list.list FROM card_list)
                ) AS c
                JOIN card_secs AS cs ON cs.card = c.card
            ) AS cards
"#);
