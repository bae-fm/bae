//! Reads of the candidate state that this domain's tests and the import's
//! tests check.

#[cfg(test)]
use crate::CandidateKey;

// Whether the stored draft of `:content_hash` is blank and whether it is
// valid.
#[cfg(test)]
coven::read!(
    writer StoredDraftFlags(content_hash: String) { draft_blank: bool, draft_valid: bool },
    r#"
    SELECT edit.draft_blank, edit.draft_valid
    FROM import_candidate_edit AS edit
    WHERE edit.content_hash = :content_hash
"#
);

// The run that the search of `:key` is on.
#[cfg(test)]
coven::read!(
    writer SearchRunOf(key: CandidateKey),
    r#"
    SELECT search.search_run
    FROM import_candidate_search AS search
    WHERE search.candidate_key = :key
"#
);

// Every release of the search `:run`, in list order.
#[cfg(test)]
coven::read!(
    writer SearchLayout(run: i64),
    r#"
    SELECT found.release_id, found.pressing, found.card, found.record_order
    FROM import_candidate_match AS found
    JOIN import_candidate_search_source AS part
      ON part.search_run = found.search_run AND part.catalog = found.source
    WHERE found.search_run = :run
    ORDER BY part.position, found.position
"#
);

// Each release of the search `:run` with its album links and the album link
// row that names it, as `release_id links catalog:key`, by release.
#[cfg(test)]
coven::read!(
    writer SearchAlbumLinks(run: i64) { line: String },
    r#"
    SELECT found.release_id || ' ' || found.album_links || ' '
               || COALESCE(link.catalog || ':' || link.key, '') AS line
    FROM import_candidate_match AS found
    LEFT JOIN import_candidate_match_album_link AS link ON link.match_id = found.match_id
    WHERE found.search_run = :run
    ORDER BY found.release_id
"#
);

// The text of the search `:run`'s cards, as `card kind ordinal text`, in
// order.
#[cfg(test)]
coven::read!(
    writer SearchCardText(run: i64) { line: String },
    r#"
    SELECT named.card || ' ' || named.kind || ' ' || named.ordinal || ' ' || named.text AS line
    FROM import_candidate_card_text AS named
    JOIN import_candidate_card_list AS list ON list.list = named.list
    WHERE list.search_run = :run
    ORDER BY named.card, named.kind, named.ordinal
"#
);

// What the search `:run`'s rows list, as `pressing fact line ordinal value
// catalogs`, in order.
#[cfg(test)]
coven::read!(
    writer SearchRowStatements(run: i64) { line: String },
    r#"
    SELECT stated.pressing || ' ' || stated.fact || ' ' || stated.line || ' '
               || stated.ordinal || ' ' || stated.value || ' ' || stated.catalogs AS line
    FROM import_candidate_row_statement AS stated
    JOIN import_candidate_card_list AS list ON list.list = stated.list
    WHERE list.search_run = :run
    ORDER BY stated.pressing, stated.fact, stated.line, stated.ordinal
"#
);

// Each release of the verdict of `:content_hash`, in position order, with
// how it agrees with the folder's catalog number, where it is laid out and
// which listing it is in.
#[cfg(test)]
coven::read!(
    writer VerdictMatches(content_hash: String) { agrees_catalog: i64 },
    r#"
    SELECT found.release_id, found.agrees_catalog, found.card, found.record_order,
        found.listing
    FROM import_candidate_match AS found
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = found.verdict_id
    WHERE verdict.content_hash = :content_hash
    ORDER BY found.position
"#
);

// What the rows of the verdict of `:content_hash` list, as `listing pressing
// fact value`, in order.
#[cfg(test)]
coven::read!(
    writer VerdictRowStatements(content_hash: String) { line: String },
    r#"
    SELECT stated.listing || ' ' || stated.pressing || ' ' || stated.fact || ' '
               || stated.value AS line
    FROM import_candidate_row_statement AS stated
    JOIN import_candidate_card_list AS list ON list.list = stated.list
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = list.verdict_id
    WHERE verdict.content_hash = :content_hash
    ORDER BY stated.listing, stated.pressing, stated.fact, stated.line, stated.ordinal
"#
);

// Where each release the ledger of `:content_hash`'s verdict found is laid
// out, as `release_id pressing card record_order`, in the ledger's order.
#[cfg(test)]
coven::read!(
    writer LedgerLayout(content_hash: String) { line: String },
    r#"
    SELECT found.release_id || ' ' || lookup.pressing || ' ' || lookup.card || ' '
               || lookup.record_order AS line
    FROM import_candidate_lookup_release AS lookup
    JOIN import_candidate_match AS found
      ON found.verdict_id = lookup.verdict_id AND found.position = lookup.match_position
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = lookup.verdict_id
    WHERE verdict.content_hash = :content_hash
    ORDER BY lookup.ordinal
"#
);

// The text of the cards the ledger of `:content_hash`'s verdict lists, as
// `card kind ordinal text`, in order.
#[cfg(test)]
coven::read!(
    writer LedgerCardText(content_hash: String) { line: String },
    r#"
    SELECT named.card || ' ' || named.kind || ' ' || named.ordinal || ' ' || named.text AS line
    FROM import_candidate_card_text AS named
    JOIN import_candidate_card_list AS list ON list.list = named.list
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = list.verdict_id
    WHERE verdict.content_hash = :content_hash AND list.step IS NOT NULL
    ORDER BY named.card, named.kind, named.ordinal
"#
);

// What the rows of the ledger of `:content_hash`'s verdict list, as `pressing
// fact line ordinal value catalogs`, in order.
#[cfg(test)]
coven::read!(
    writer LedgerRowStatements(content_hash: String) { line: String },
    r#"
    SELECT stated.pressing || ' ' || stated.fact || ' ' || stated.line || ' '
               || stated.ordinal || ' ' || stated.value || ' ' || stated.catalogs AS line
    FROM import_candidate_row_statement AS stated
    JOIN import_candidate_card_list AS list ON list.list = stated.list
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = list.verdict_id
    WHERE verdict.content_hash = :content_hash AND list.step IS NOT NULL
    ORDER BY stated.pressing, stated.fact, stated.line, stated.ordinal
"#
);

// How many candidate states `:content_hash` has.
#[cfg(test)]
coven::read!(
    writer CandidateStateCount(content_hash: String) { held: i64 },
    r#"
    SELECT COUNT(*) AS held
    FROM import_candidate_state AS state
    WHERE state.content_hash = :content_hash
"#
);

// How many rows each table of the identify run `:run` holds: the verdict and
// signals headers by their run, and the rows under them through the header
// they name.
#[cfg(test)]
coven::read!(
    writer RunRowCounts(run: i64) { table_name: String, held: i64 },
    r#"
    SELECT 'import_candidate_verdict' AS table_name, COUNT(*) AS held
    FROM import_candidate_verdict AS verdict WHERE verdict.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_signals', COUNT(*)
    FROM import_candidate_signals AS signals WHERE signals.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_signal_value', COUNT(*)
    FROM import_candidate_signal_value AS owned
    JOIN import_candidate_signals AS header ON header.signals_id = owned.signals_id
    WHERE header.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_text_line', COUNT(*)
    FROM import_candidate_text_line AS owned
    JOIN import_candidate_signals AS header ON header.signals_id = owned.signals_id
    WHERE header.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_match', COUNT(*)
    FROM import_candidate_match AS owned
    JOIN import_candidate_verdict AS header ON header.verdict_id = owned.verdict_id
    WHERE header.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_lookup_value', COUNT(*)
    FROM import_candidate_lookup_value AS owned
    JOIN import_candidate_verdict AS header ON header.verdict_id = owned.verdict_id
    WHERE header.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_lookup', COUNT(*)
    FROM import_candidate_lookup AS owned
    JOIN import_candidate_verdict AS header ON header.verdict_id = owned.verdict_id
    WHERE header.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_lookup_release', COUNT(*)
    FROM import_candidate_lookup_release AS owned
    JOIN import_candidate_verdict AS header ON header.verdict_id = owned.verdict_id
    WHERE header.identify_run = :run
    UNION ALL
    SELECT 'import_candidate_lookup_provider', COUNT(*)
    FROM import_candidate_lookup_provider AS owned
    JOIN import_candidate_verdict AS header ON header.verdict_id = owned.verdict_id
    WHERE header.identify_run = :run
"#
);

// Each lookup cell waiting for or running its lookup, with the run that asked
// it and the run of its verdict.
#[cfg(test)]
coven::read!(
    writer AskingCells,
    r#"
    SELECT cell.answer, cell.asked_by, verdict.identify_run
    FROM import_candidate_lookup AS cell
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = cell.verdict_id
    WHERE cell.answer IN ('queued', 'looking_up')
"#
);

// What every identification batch row counts: the folders ended and the rows.
#[cfg(test)]
coven::read!(
    writer BatchTotals { ended: u32, batches: u32 },
    r#"
    SELECT COALESCE(SUM(batch.ended), 0) AS ended, COUNT(*) AS batches
    FROM import_identification_batch AS batch
"#
);

// How many candidates wait for, run or write an identification.
#[cfg(test)]
coven::read!(
    writer IdentificationsInFlight { held: u32 },
    r#"
    SELECT COUNT(*) AS held
    FROM import_candidate_standing AS standing
    WHERE standing.identification IN ('queued', 'running', 'finalizing', 'reading_pick')
"#
);

// How many stored releases the catalog `:source` returned.
#[cfg(test)]
coven::read!(
    writer MatchesFrom(source: String) { held: i64 },
    r#"
    SELECT COUNT(*) AS held
    FROM import_candidate_match AS found
    WHERE found.source = :source
"#
);

// How many rows each table that an import spends holds under
// `:content_hash`: the verdict's rows through the verdict they name, and the
// others by their content hash.
#[cfg(test)]
coven::read!(
    writer SpentRowCounts(content_hash: String) { table_name: String, held: i64 },
    r#"
    SELECT 'import_candidate_match' AS table_name, COUNT(*) AS held
    FROM import_candidate_match AS owned
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = owned.verdict_id
    WHERE verdict.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_lookup_provider', COUNT(*)
    FROM import_candidate_lookup_provider AS owned
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = owned.verdict_id
    WHERE verdict.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_lookup_value', COUNT(*)
    FROM import_candidate_lookup_value AS owned
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = owned.verdict_id
    WHERE verdict.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_lookup', COUNT(*)
    FROM import_candidate_lookup AS owned
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = owned.verdict_id
    WHERE verdict.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_lookup_release', COUNT(*)
    FROM import_candidate_lookup_release AS owned
    JOIN import_candidate_verdict AS verdict ON verdict.verdict_id = owned.verdict_id
    WHERE verdict.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_verdict', COUNT(*)
    FROM import_candidate_verdict AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_lookup_choices', COUNT(*)
    FROM import_candidate_lookup_choices AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_chosen_catalog', COUNT(*)
    FROM import_candidate_chosen_catalog AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_pane_failure', COUNT(*)
    FROM import_candidate_pane_failure AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_album_pick', COUNT(*)
    FROM import_candidate_album_pick AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_edit', COUNT(*)
    FROM import_candidate_edit AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_album_artist_assignment', COUNT(*)
    FROM import_candidate_album_artist_assignment AS owned
    WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_track', COUNT(*)
    FROM import_candidate_track AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_release_link', COUNT(*)
    FROM import_candidate_release_link AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_release_link_album', COUNT(*)
    FROM import_candidate_release_link_album AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_claimed_record', COUNT(*)
    FROM import_candidate_claimed_record AS owned WHERE owned.content_hash = :content_hash
    UNION ALL
    SELECT 'import_candidate_cover', COUNT(*)
    FROM import_candidate_cover AS owned WHERE owned.content_hash = :content_hash
"#
);

// How many draft and cover rows `:content_hash` has.
#[cfg(test)]
coven::read!(
    writer DraftRowCounts(content_hash: String) { edits: i64, covers: i64 },
    r#"
    SELECT (SELECT COUNT(*) FROM import_candidate_edit AS edit
            WHERE edit.content_hash = :content_hash) AS edits,
        (SELECT COUNT(*) FROM import_candidate_cover AS cover
         WHERE cover.content_hash = :content_hash) AS covers
"#
);

/// How many draft and cover rows `hash` has in `db`.
#[cfg(test)]
pub(crate) async fn draft_rows(db: &bae_store::Store, hash: &str) -> DraftRowCountsRow {
    let content_hash = hash.to_string();
    db.read(move |sql| bae_store::statement_row(&sql, &DraftRowCountsParams { content_hash }))
        .await
        .unwrap()
}

// Whether a candidate's link, or a partner of one, claims the release
// `:release_id` of `:source`.
#[cfg(test)]
coven::read!(
    writer ClaimedByLink(source: String, release_id: String) { claimed: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM import_candidate_release_link AS link
                   WHERE link.source = :source AND link.release_id = :release_id)
        OR EXISTS (SELECT 1 FROM import_candidate_release_link_partner AS partner
                   WHERE partner.source = :source AND partner.release_id = :release_id)
        AS claimed
"#
);

// How many candidate folders and candidate states are stored.
#[cfg(test)]
coven::read!(
    writer FolderStateCounts { folders: i64, states: i64 },
    r#"
    SELECT (SELECT COUNT(*) FROM import_candidate_folder AS folder) AS folders,
        (SELECT COUNT(*) FROM import_candidate_state AS state) AS states
"#
);

// How many releases are stored for any verdict or search.
#[cfg(test)]
coven::read!(
    writer MatchCount { held: i64 },
    "SELECT COUNT(*) AS held FROM import_candidate_match AS found"
);
