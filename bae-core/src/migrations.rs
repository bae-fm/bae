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
    ]
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
