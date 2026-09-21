//! bae's synced-schema migration ladder; coven applies versions above `PRAGMA user_version` at open.

const IMPORT_METADATA_SEEDS_SQL: &str = include_str!("../migrations/002_import_metadata_seeds.sql");
const METADATA_DRAFTS_AND_PROVENANCE_SQL: &str =
    include_str!("../migrations/003_metadata_drafts_and_provenance.sql");
const IMPORT_SOURCE_AUDIO_FACTS_SQL: &str =
    include_str!("../migrations/005_import_source_audio_facts.sql");
const SCAN_METADATA_IDENTITY_SQL: &str =
    include_str!("../migrations/006_scan_metadata_identity.sql");
const VERSION_ONE_FILE_TAG_TRACK_PREFIX: &str = "unknown-track-";
const FILE_TAG_TRACK_PREFIX: &str = "file-tag-track-";

struct VersionOneTrackEdit {
    content_hash: String,
    old_track_id: String,
    track_id: String,
    artist_names: Option<Vec<String>>,
}

/// The ordered migration ladder. Versions are 1-based and contiguous.
pub fn all() -> Vec<coven::Migration> {
    vec![
        coven::Migration::sql(1, "initial", include_str!("../migrations/001_initial.sql")),
        coven::Migration::run(2, "import_metadata_seeds", migrate_import_metadata_seeds),
        coven::Migration::sql(
            3,
            "metadata_drafts_and_provenance",
            METADATA_DRAFTS_AND_PROVENANCE_SQL,
        ),
        coven::Migration::sql(
            4,
            "import_artist_identity_conflicts",
            include_str!("../migrations/004_import_artist_identity_conflicts.sql"),
        ),
        coven::Migration::sql(
            5,
            "import_source_audio_facts",
            IMPORT_SOURCE_AUDIO_FACTS_SQL,
        ),
        coven::Migration::sql(6, "scan_metadata_identity", SCAN_METADATA_IDENTITY_SQL),
        coven::Migration::sql(
            7,
            "identify_failures",
            include_str!("../migrations/007_identify_failures.sql"),
        ),
        coven::Migration::sql(
            8,
            "import_candidate_album_year",
            include_str!("../migrations/008_import_candidate_album_year.sql"),
        ),
        coven::Migration::sql(
            9,
            "import_prepared_assets",
            include_str!("../migrations/009_import_prepared_assets.sql"),
        ),
        coven::Migration::sql(
            10,
            "import_candidate_watched_roots",
            include_str!("../migrations/010_import_candidate_watched_roots.sql"),
        ),
        coven::Migration::sql(
            11,
            "track_mapping_named_by_source",
            include_str!("../migrations/011_track_mapping_named_by_source.sql"),
        ),
        coven::Migration::sql(
            12,
            "import_candidate_match_barcode",
            include_str!("../migrations/012_import_candidate_match_barcode.sql"),
        ),
        coven::Migration::sql(
            13,
            "import_candidate_provenance_partners",
            include_str!("../migrations/013_import_candidate_provenance_partners.sql"),
        ),
        coven::Migration::sql(
            14,
            "cue_binding_resolution",
            include_str!("../migrations/014_cue_binding_resolution.sql"),
        ),
        coven::Migration::sql(
            15,
            "scan_sheet_audio_file",
            include_str!("../migrations/015_scan_sheet_audio_file.sql"),
        ),
        coven::Migration::sql(
            16,
            "candidate_dates",
            include_str!("../migrations/016_candidate_dates.sql"),
        ),
        coven::Migration::sql(
            17,
            "import_candidate_session",
            include_str!("../migrations/017_import_candidate_session.sql"),
        ),
        coven::Migration::sql(
            18,
            "candidate_combinations",
            include_str!("../migrations/018_candidate_combinations.sql"),
        ),
        coven::Migration::sql(
            19,
            "import_candidate_track",
            include_str!("../migrations/019_import_candidate_track.sql"),
        ),
        coven::Migration::sql(
            20,
            "candidate_verdict_and_provenance",
            include_str!("../migrations/020_candidate_verdict_and_provenance.sql"),
        ),
        coven::Migration::sql(
            21,
            "signal_value_region",
            include_str!("../migrations/021_signal_value_region.sql"),
        ),
        coven::Migration::sql(
            22,
            "candidate_narrowed_out_matches",
            include_str!("../migrations/022_candidate_narrowed_out_matches.sql"),
        ),
        coven::Migration::sql(
            23,
            "candidate_lookup_choices",
            include_str!("../migrations/023_candidate_lookup_choices.sql"),
        ),
        coven::Migration::sql(
            24,
            "candidate_verdict_ledger",
            include_str!("../migrations/024_candidate_verdict_ledger.sql"),
        ),
        coven::Migration::sql(
            25,
            "candidate_text_pool",
            include_str!("../migrations/025_candidate_text_pool.sql"),
        ),
        coven::Migration::sql(
            26,
            "candidate_discounted_catalog",
            include_str!("../migrations/026_candidate_discounted_catalog.sql"),
        ),
        coven::Migration::sql(
            27,
            "scan_candidate_drops_initial_source",
            include_str!("../migrations/027_scan_candidate_drops_initial_source.sql"),
        ),
        coven::Migration::sql(
            28,
            "candidate_session_drops_file_tags",
            include_str!("../migrations/028_candidate_session_drops_file_tags.sql"),
        ),
        coven::Migration::sql(
            29,
            "candidate_combination_drops_track_order",
            include_str!("../migrations/029_candidate_combination_drops_track_order.sql"),
        ),
        coven::Migration::sql(
            30,
            "candidate_excluded_barcode",
            include_str!("../migrations/030_candidate_excluded_barcode.sql"),
        ),
        coven::Migration::sql(
            31,
            "release_records",
            include_str!("../migrations/031_release_records.sql"),
        ),
        coven::Migration::sql(
            32,
            "field_origins",
            include_str!("../migrations/032_field_origins.sql"),
        ),
        coven::Migration::sql(
            33,
            "release_marks",
            include_str!("../migrations/033_release_marks.sql"),
        ),
        coven::Migration::sql(
            34,
            "release_verification",
            include_str!("../migrations/034_release_verification.sql"),
        ),
        coven::Migration::sql(
            35,
            "release_identified_by",
            include_str!("../migrations/035_release_identified_by.sql"),
        ),
        coven::Migration::sql(
            36,
            "release_mark_corroboration",
            include_str!("../migrations/036_release_mark_corroboration.sql"),
        ),
        coven::Migration::sql(
            37,
            "audio_backed_drafts",
            include_str!("../migrations/037_audio_backed_drafts.sql"),
        ),
        coven::Migration::sql(
            38,
            "track_order_and_unknown_sides",
            include_str!("../migrations/038_track_order_and_unknown_sides.sql"),
        ),
        coven::Migration::sql(
            39,
            "cue_reference_bindings",
            include_str!("../migrations/039_cue_reference_bindings.sql"),
        ),
        coven::Migration::run(
            40,
            "applied_source_documents",
            migrate_applied_source_documents,
        ),
        coven::Migration::sql(
            41,
            "remove_field_origins",
            include_str!("../migrations/041_remove_field_origins.sql"),
        )
        .changesets(crate::migration_changesets::remove_field_origins()),
        coven::Migration::sql(
            42,
            "record_kinds",
            include_str!("../migrations/042_record_kinds.sql"),
        )
        .changesets(crate::migration_changesets::record_kinds()),
        coven::Migration::run(
            43,
            "applied_source_partners",
            migrate_applied_source_partners,
        ),
        coven::Migration::run(44, "match_evidence", migrate_match_evidence),
        coven::Migration::run(
            45,
            "candidate_folder_covers",
            migrate_candidate_folder_covers,
        ),
        coven::Migration::run(46, "match_pressings", migrate_match_pressings),
    ]
}

/// Record the pressing row each stored match belongs to. The rows a run built
/// are its own answer, and re-forming them from one of its two lists is a
/// different one; the matches already stored have nothing that says what
/// their run built, so the grouping as it stands gives them their rows.
fn migrate_match_pressings(sql: &coven::MigrationContext<'_>) -> Result<(), coven::DbError> {
    let counted = |sql: &coven::MigrationContext<'_>| {
        sql.query_row("SELECT COUNT(*) FROM import_candidate_match", [], |row| {
            row.get::<_, i64>(0)
        })
    };
    let before = counted(sql)?;
    // The row each stored match belongs to, answered before the table is
    // rebuilt around the column that records it. Folder scanning is
    // desktop-only, so a mobile store holds no match to answer for.
    sql.execute_batch(
        "CREATE TEMP TABLE match_pressing (
             content_hash TEXT NOT NULL,
             position     INTEGER NOT NULL,
             pressing     INTEGER NOT NULL CHECK (pressing >= 0),
             PRIMARY KEY (content_hash, position)
         );",
    )?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    crate::db::Database::fill_match_pressings(sql)?;
    sql.execute_batch(include_str!("../migrations/046_match_pressings.sql"))?;
    let after = counted(sql)?;
    if before != after {
        return Err(coven::DbError::Message(format!(
            "{before} stored matches were given a pressing row and {after} crossed the rebuild"
        )));
    }
    Ok(())
}

/// Store the cover each candidate's folder gives it, for the candidates
/// scanned before a scan stored one. Folder scanning is desktop-only, so a
/// mobile store holds no candidate to fill one for.
fn migrate_candidate_folder_covers(
    _sql: &coven::MigrationContext<'_>,
) -> Result<(), coven::DbError> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    crate::db::Database::fill_candidate_folder_covers(_sql)?;
    Ok(())
}

fn migrate_import_metadata_seeds(sql: &coven::MigrationContext<'_>) -> Result<(), coven::DbError> {
    let album_artist_edits = sql.query(
        "SELECT content_hash, album_artist_text FROM import_candidate_edit \
         WHERE album_artist_text IS NOT NULL ORDER BY content_hash",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let track_edits = sql.query(
        "SELECT edit.content_hash, edit.track_id, state.pick_kind, edit.artist_text \
         FROM import_candidate_track_edit AS edit \
         JOIN import_candidate_state AS state USING (content_hash) \
         ORDER BY edit.content_hash, edit.track_id",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;

    let album_artist_edits = parse_v1_artist_edits(album_artist_edits, "album")?;
    let track_edits = track_edits
        .into_iter()
        .map(|(content_hash, old_track_id, pick_kind, artist_text)| {
            let track_id = version_two_track_id(pick_kind.as_deref(), &old_track_id);
            let artist_names = artist_text.map(|text| parse_v1_artist_text(&text));
            VersionOneTrackEdit {
                content_hash,
                old_track_id,
                track_id,
                artist_names,
            }
        })
        .collect::<Vec<_>>();

    sql.execute_batch(IMPORT_METADATA_SEEDS_SQL)?;

    for edit in &track_edits {
        if edit.old_track_id != edit.track_id {
            sql.execute(
                "UPDATE import_candidate_track_edit SET track_id = ? \
                 WHERE content_hash = ? AND track_id = ?",
                coven::rusqlite::params![edit.track_id, edit.content_hash, edit.old_track_id],
            )?;
        }
    }

    for (content_hash, names) in album_artist_edits {
        for (position, name) in names.into_iter().enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_album_artist_assignment \
                 (content_hash, position, assignment_kind, artist_id, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id) \
                 VALUES (?, ?, 'new', NULL, ?, NULL, NULL, NULL)",
                coven::rusqlite::params![content_hash, position as i64, name],
            )?;
        }
    }
    for edit in track_edits {
        if let Some(names) = edit.artist_names {
            for (position, name) in names.into_iter().enumerate() {
                sql.execute(
                    "INSERT INTO import_candidate_track_artist_assignment \
                     (content_hash, track_id, position, assignment_kind, artist_id, name, sort_name, \
                      musicbrainz_artist_id, discogs_artist_id) \
                     VALUES (?, ?, ?, 'new', NULL, ?, NULL, NULL, NULL)",
                    coven::rusqlite::params![edit.content_hash, edit.track_id, position as i64, name],
                )?;
            }
        }
    }

    sql.execute_batch(
        "DROP TABLE import_candidate_signal_value_v1; \
         DROP TABLE import_candidate_match_v1; \
         DROP TABLE import_candidate_file_edit_v1; \
         DROP TABLE import_candidate_file_duration_v1; \
         DROP TABLE import_candidate_failure_v1; \
         DROP TABLE import_candidate_cover_v1; \
         DROP TABLE import_candidate_edit_v1; \
         DROP TABLE import_candidate_track_edit_v1; \
         DROP TABLE import_candidate_signals_v1; \
         DROP TABLE import_candidate_state_v1;",
    )?;

    let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    if !violations.is_empty() {
        return Err(coven::DbError::Message(format!(
            "migration 2 produced foreign-key violations: {violations:?}"
        )));
    }
    Ok(())
}

fn version_two_track_id(pick_kind: Option<&str>, track_id: &str) -> String {
    match (
        pick_kind,
        track_id.strip_prefix(VERSION_ONE_FILE_TAG_TRACK_PREFIX),
    ) {
        (Some("unknown"), Some(index)) => format!("{FILE_TAG_TRACK_PREFIX}{index}"),
        _ => track_id.to_string(),
    }
}

fn parse_v1_artist_edits(
    edits: Vec<(String, String)>,
    field: &str,
) -> Result<Vec<(String, Vec<String>)>, coven::DbError> {
    edits
        .into_iter()
        .map(|(content_hash, text)| {
            let names = parse_v1_artist_text(&text);
            if names.is_empty() {
                return Err(coven::DbError::Message(format!(
                    "candidate {content_hash} has an empty version-1 {field} artist edit"
                )));
            }
            Ok((content_hash, names))
        })
        .collect()
}

fn parse_v1_artist_text(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "migrations_tests.rs"]
mod tests;

fn migrate_applied_source_documents(
    sql: &coven::MigrationContext<'_>,
) -> Result<(), coven::DbError> {
    sql.execute_batch(include_str!(
        "../migrations/040_applied_source_documents.sql"
    ))?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    crate::db::Database::migrate_applied_sources(sql)?;
    Ok(())
}

/// Rebuild the stored matches around every barcode, the media each record
/// described, and the releases its document linked. The SQL rebuilds the
/// table and copies the one stored barcode; the medium rows and the ledger
/// need the same split the Rust side wrote them with, so they are done here.
fn migrate_match_evidence(sql: &coven::MigrationContext<'_>) -> Result<(), coven::DbError> {
    sql.execute_batch(include_str!("../migrations/044_match_evidence.sql"))?;
    let described = sql.query(
        "SELECT content_hash, position, source, format FROM import_candidate_match \
         WHERE format IS NOT NULL ORDER BY content_hash, position",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    for (content_hash, position, source, format) in described {
        for (ordinal, descriptor) in match_format_descriptors(&source, &format)?.enumerate() {
            sql.execute(
                "INSERT INTO import_candidate_match_medium \
                     (content_hash, position, media_kind, ordinal, format) \
                 VALUES (?, ?, 'descriptors', ?, ?)",
                coven::rusqlite::params![content_hash, position, ordinal as i64, descriptor],
            )?;
        }
    }
    let ledgers = sql.query(
        "SELECT content_hash, ledger_json FROM import_candidate_verdict \
         WHERE ledger_json IS NOT NULL ORDER BY content_hash",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    for (content_hash, json) in ledgers {
        let mut ledger: serde_json::Value = serde_json::from_str(&json).map_err(|error| {
            coven::DbError::Message(format!(
                "the identify ledger for {content_hash} is unreadable: {error}"
            ))
        })?;
        rewrite_ledger_results(&mut ledger)?;
        sql.execute(
            "UPDATE import_candidate_verdict SET ledger_json = ? WHERE content_hash = ?",
            coven::rusqlite::params![ledger.to_string(), content_hash],
        )?;
    }
    Ok(())
}

/// The descriptors a stored format string was written from: a Discogs
/// result's format names and qualifiers were joined with ", ", and a
/// MusicBrainz result's is the one format of the medium its disc ID matched.
fn match_format_descriptors<'a>(
    source: &str,
    format: &'a str,
) -> Result<Box<dyn Iterator<Item = &'a str> + 'a>, coven::DbError> {
    match source {
        "discogs" => Ok(Box::new(format.split(", "))),
        "musicbrainz" => Ok(Box::new(std::iter::once(format))),
        other => Err(coven::DbError::Message(format!(
            "a stored match names the catalog {other:?}, which answers no lookups"
        ))),
    }
}

/// The ledger stores the results a lookup found inside its cards. Each result
/// object gains the same shape the table did: its one barcode becomes the
/// list of barcodes, its format describes its media, and it links nothing.
fn rewrite_ledger_results(value: &mut serde_json::Value) -> Result<(), coven::DbError> {
    match value {
        serde_json::Value::Object(object) => {
            if is_stored_result(object) {
                let barcode = object
                    .remove("barcode")
                    .expect("a stored result states a barcode column");
                let barcodes = match barcode {
                    serde_json::Value::Null => Vec::new(),
                    code @ serde_json::Value::String(_) => vec![code],
                    other => {
                        return Err(coven::DbError::Message(format!(
                            "a stored result's barcode is {other}, not text"
                        )))
                    }
                };
                // The ledger writes a catalog as its variant name, the table
                // as its column value.
                let source = match object["source"].as_str() {
                    Some("MusicBrainz") => "musicbrainz",
                    Some("Discogs") => "discogs",
                    other => {
                        return Err(coven::DbError::Message(format!(
                            "a stored result names the catalog {other:?}, which answers no lookups"
                        )))
                    }
                };
                let media = match object["format"].as_str() {
                    None => serde_json::Value::String("Undescribed".to_string()),
                    Some(format) => serde_json::json!({
                        "Descriptors": match_format_descriptors(source, format)?.collect::<Vec<_>>()
                    }),
                };
                object.insert("barcodes".to_string(), serde_json::Value::Array(barcodes));
                object.insert("media".to_string(), media);
                object.insert("links".to_string(), serde_json::Value::Array(Vec::new()));
            }
            for value in object.values_mut() {
                rewrite_ledger_results(value)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                rewrite_ledger_results(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Whether a ledger object is one stored result: the one object shape in a
/// ledger that names a release with a source, a barcode and a tracklist.
fn is_stored_result(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    [
        "source",
        "release_id",
        "barcode",
        "source_tracks",
        "source_group_id",
    ]
    .iter()
    .all(|key| object.contains_key(*key))
}

fn migrate_applied_source_partners(
    _sql: &coven::MigrationContext<'_>,
) -> Result<(), coven::DbError> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    crate::db::Database::migrate_applied_source_partners(_sql)?;
    Ok(())
}
