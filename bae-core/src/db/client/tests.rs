use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScannedFile,
};

// Fixture row ids, UUIDs because coven refuses any other synced primary key.
const ALBUM_1: &str = "9644b84d-94b2-4b3b-863a-d6583931920c";
const ALBUM_1999: &str = "88f57246-3e65-4eb9-8d36-ee8d40326cfc";
const ALBUM_2001_LOWER: &str = "88183677-683b-485e-8224-f6a328c233c7";
const ALBUM_2001_UPPER: &str = "a663cff7-fad7-45b1-8469-5f77af82ddb8";
const ALBUM_A: &str = "a67c03ad-425f-45e9-8279-0144c852aaa5";
const ALBUM_JUNCTION: &str = "7e6f42e7-8952-48e6-89bf-d1bcc611176d";
const ALBUM_NEW: &str = "7d40ec33-80aa-4ab5-8010-78b55943ad81";
const ALBUM_NULL: &str = "c6648d5a-617e-4b69-87da-b7f1c4fb5e65";
const ALBUM_OLD: &str = "d80af162-0f69-4558-803e-742f4089d486";
const ALBUM_PERCENT: &str = "7e9948c4-f2d0-4a73-8e5c-a885eda086ff";
const ALBUM_UNDERSCORE: &str = "2dd55a3f-3208-4faf-8737-453f474074cb";
const ARTIST_1: &str = "6c441836-aef7-4239-8a84-5336c4cce52c";
const ARTIST_A: &str = "d7d8141f-54ff-467d-8b60-4f34a4d2e528";
const ARTIST_ABSENT: &str = "78420eae-1cd1-4a36-87ae-2a5556aa52aa";
const ARTIST_ALBUM: &str = "85f70840-aba5-4eb9-8e1a-0d319e53b798";
const ARTIST_B: &str = "38fc314c-c130-4120-8ca9-38b870ccef3a";
const ARTIST_C: &str = "1b4bafc9-0ece-4538-833e-4ff52feb6ef0";
const ARTIST_COMPOSER: &str = "5412b7ad-bdc1-4561-8985-b6d6ef8a2880";
const ARTIST_EXTRA: &str = "7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8";
const ARTIST_PRIMARY: &str = "7cdf9a34-0746-472b-8c68-0a669c11f2f1";
const ARTIST_SOLO: &str = "49549823-0e72-4747-891e-ee50e1611e3a";
const ARTIST_VARIOUS: &str = "f862abf2-3b15-4518-889b-1996d7100201";
const ARTIST_WORK_ONLY: &str = "b96d8066-777d-408d-8ae4-ed58c767e40c";
const BLOB_1: &str = "222d362a-5ce1-45ff-8a54-341cde525c2c";
const BLOB_2: &str = "b1b46178-280d-48d4-86b3-62b31c040179";
const COMPOSER_A: &str = "5dcc4999-03bd-42cc-8d14-8bf0a05effa3";
const COMPOSER_B: &str = "2b748d47-e5b7-4c40-8716-1e608b9dfc3d";
const COMPOSER_C: &str = "80cd3a5e-7fb7-4766-8ec3-d8e86575743b";
const COMPOSER_SOLO: &str = "4d93d615-4549-45d9-81d9-644f079d59bf";
const ENTRY_A: &str = "e2ebff4e-4ed0-4a73-88ed-93453a79b463";
const FILE_NEW: &str = "48804352-31c6-4a7c-8f44-9ac4cc62abdf";
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882";
const REL_NEW: &str = "f3078482-3f35-4019-8ade-a04971532682";
const REL_OLD: &str = "3113dc59-d689-4c8c-86e9-4a3ae1565563";
const REL_ONE: &str = "35fa3546-ff78-4214-857a-d323014e4e2c";
const REL_TWO: &str = "6f389f38-00da-41c6-8dbf-365b1f7823fe";
const RELEASE_1: &str = "c0218676-4c47-4eb7-8d65-57a8d328c3d1";
const RELEASE_A: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";
const RELEASE_B: &str = "64e79a1f-404a-4c34-809a-a3cb44bf1942";
const RELEASE_LONELY: &str = "fcf4be32-159f-4790-87a1-697700a74462";
const RELEASE_OTHER: &str = "ce596bd7-be97-4416-8b6d-47f315bae466";
const RELEASE_ROLE_A: &str = "9b72bbbf-621e-41ca-8930-1623b643a20d";
const RELEASE_Z: &str = "8aa66d48-65a0-42e4-8c1d-e7481e8c1861";
const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc";
const TRACK_B: &str = "04676261-1659-47b1-879c-2947c52f4a8d";
const TRACK_LONELY: &str = "03c41035-ce18-4fa0-8e83-c446df26a551";
const TRACK_NEW: &str = "d28100a4-a355-47d3-8d5d-5a7b80bc66fd";
const TRACK_OTHER: &str = "69e67928-545a-4dcf-8ae7-ef7778331231";
const TRACK_PERCENT: &str = "4dc8cde9-15fb-470d-802c-b7e5f1ccc63d";
const TRACK_ROLE_A: &str = "fa0c8483-f09a-4b69-8903-b1ebcdc31322";
const TRACK_UNDERSCORE: &str = "b2930937-dae6-4719-8150-aa61422eeeac";
const TRACK_WORK_A: &str = "d410a973-6a19-4ad3-87d8-b0c8c13d6015";
const WORK_A: &str = "432c8996-8af0-43dc-868a-822a256f65c4";
const WORK_CHILD_A: &str = "f63d8e66-6a81-4a67-8005-1fbe870f27eb";
const WORK_PARENT_A: &str = "6b05af7a-ee0c-4f12-8938-1d5536697271";

/// A database in a fresh temp directory, under the real clock. The `TempDir`
/// owns the file, so it must outlive the handle.
async fn temp_db() -> (super::Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = super::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// The folder at `name`, a `/`-separated path below `root`, spelled the way
/// the scanner spells it on this host.
fn folder_below(root: &str, name: &str) -> std::path::PathBuf {
    name.split('/')
        .fold(std::path::PathBuf::from(root), |folder, part| {
            folder.join(part)
        })
}

/// A scanned folder at `name` below watched root `root` holding `files`, as the
/// scanner produces it for a folder whose file decisions nobody has edited.
fn candidate_with(
    root: &str,
    name: &str,
    files: CategorizedFiles,
    scope: ReleaseFileScope,
) -> FolderCandidate {
    let path = folder_below(root, name);
    FolderCandidate {
        path: path.clone(),
        file_root: path,
        name: name.to_string(),
        files,
        watched_folder_path: root.to_string(),
        scope,
        file_edit_revision: 0,
        display_path: name.to_string(),
        grouping: None,
    }
}

/// `candidate_with` for the folder most import fixtures want: one bound FLAC,
/// scanned recursively.
fn candidate(root: &str, name: &str) -> FolderCandidate {
    candidate_with(
        root,
        name,
        CategorizedFiles {
            files: vec![CandidateFile {
                proposed_audio: true,
                file: ScannedFile::new(
                    folder_below(root, name).join("01.flac"),
                    "01.flac".to_string(),
                    1_000,
                    1,
                )
                .with_test_flac_audio(),
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    )
}

/// The instant the import fixtures pin their clock to.
fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// `temp_db` under that pinned clock instead of the system one.
async fn empty_db() -> (super::Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = super::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::FixedClock(fixed_now())),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// `empty_db` plus a watched-folder root that exists on disk.
async fn watched_root() -> (super::Database, tempfile::TempDir, String) {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap().to_string();
    (db, tmp, root)
}

/// `entries` resolved through the queue catalog query's first read, as the
/// live queue resolves them.
async fn queue_items(
    db: &super::Database,
    entries: &[crate::playback::QueueEntry],
) -> Vec<crate::queue::QueueItem> {
    let mut live =
        db.subscribe_queue_catalog(super::QueueCatalogRequest::for_entries(entries, None));
    live.next().await.into_result().unwrap().items(entries)
}

/// Run one statement against the test database, panicking if it fails.
async fn exec(db: &super::Database, sql: &str, args: &[&str]) {
    let sql = sql.to_string();
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    db.call(move |conn| {
        conn.execute(&sql, coven::rusqlite::params_from_iter(args))
            .map(|_| ())
            .map_err(coven::DbError::from)
    })
    .await
    .unwrap();
}

/// `exec` for a multi-statement script, which takes no bound parameters.
async fn exec_batch(db: &super::Database, sql: &str) {
    let sql = sql.to_string();
    db.call(move |conn| conn.execute_batch(&sql).map_err(coven::DbError::from))
        .await
        .unwrap();
}

/// `ARTIST_PRIMARY` owning `ALBUM_A`, whose primary release is `RELEASE_A`.
fn seed_artist_album_release(conn: &coven::SqlContext<'_, '_>) -> Result<(), coven::DbError> {
    conn.execute_batch(
        "
        INSERT INTO artists (id, name, name_key, _updated_at, created_at)
        VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'artist name primary', 'stamp', '2026-01-01T00:00:00Z');

        INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
        VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'stamp', '2026-01-01T00:00:00Z');

        INSERT INTO releases (id, album_id, remote, _updated_at, created_at)
        VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 1, 'stamp', '2026-01-01T00:00:00Z');
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod queue_ordering_tests;

#[cfg(test)]
mod store_file_helpers;

#[cfg(test)]
mod in_clause_chunking_tests;

#[cfg(test)]
mod aggregate_ordering_tests;

#[cfg(test)]
mod connection_boundary_tests;

#[cfg(test)]
mod readable_cloud_path_tests;

#[cfg(test)]
mod row_mapper_error_tests;

#[cfg(test)]
mod composer_mode_tests;

#[cfg(test)]
mod artist_mode_tests;

#[cfg(test)]
mod playback_state_load_tests;

#[cfg(test)]
mod import_candidate_state_tests;

#[cfg(test)]
mod import_list_tests;

#[cfg(test)]
mod release_grouping_tests;

#[cfg(test)]
mod fact_ids_tests;

#[cfg(test)]
mod queue_cover_tests;

#[cfg(test)]
mod live_query_tests;

#[cfg(test)]
mod import_list_live_query_tests;

#[cfg(test)]
mod folder_scan_live_query_tests;

#[cfg(test)]
mod file_tag_snapshot_tests;

#[cfg(test)]
mod source_release_tests;

#[cfg(test)]
mod watched_folder_tests;
