//! The folder scans' rows as the tests read them back.

// Each scanned candidate's name with the dates its folder's discovery stores,
// by name.
#[cfg(test)]
coven::read!(
    writer CandidateDates,
    r#"
    SELECT candidate.name, discovery.first_seen_at, discovery.source_date,
           discovery.source_date_kind
    FROM scan_candidate AS candidate
    JOIN folder_discovery AS discovery ON discovery.folder = candidate.folder
    ORDER BY candidate.name
"#
);

// Every folder a scan found, as its text, in the order of its bytes.
#[cfg(test)]
coven::read!(
    writer DiscoveredFolders { folder: String },
    r#"
    SELECT coven_path_text(discovery.folder) AS folder
    FROM folder_discovery AS discovery
    ORDER BY discovery.folder
"#
);

// Every folder that holds a sidecar, as its text, in the order of its bytes.
#[cfg(test)]
coven::read!(
    writer SidecarFolders { folder: String },
    r#"
    SELECT coven_path_text(sidecar.folder) AS folder
    FROM scan_sidecar AS sidecar
    ORDER BY sidecar.folder
"#
);

// The generation of the entry at `path`.
#[cfg(test)]
coven::read!(
    writer EntryGeneration(path: crate::CandidateKey),
    r#"
    SELECT candidate.generation
    FROM scan_candidate AS candidate
    WHERE candidate.path = :path
"#
);

// How many file-tag readings, and how many of their files' tags, the
// candidate at `candidate_path` under the root `root` holds.
#[cfg(test)]
coven::read!(
    writer TagReadingCounts(root: Vec<u8>, candidate_path: crate::CandidateKey) {
        readings: i64,
        facts: i64
    },
    r#"
    SELECT (SELECT count(*)
            FROM scan_candidate_tag_snapshot AS reading
            WHERE reading.watched_folder_path = :root
              AND reading.candidate_path = :candidate_path) AS readings,
           (SELECT count(*)
            FROM scan_candidate_file_tag AS fact
            WHERE fact.watched_folder_path = :root
              AND fact.candidate_path = :candidate_path) AS facts
"#
);

// Every root, by its path's bytes, with how its last scan ended.
#[cfg(test)]
coven::read!(
    writer EveryRoot,
    r#"
    SELECT root.watched_folder_path, root.generation, root.status, root.detail,
           root.unwatched_directory, root.unwatched_cause
    FROM folder_scan_roots AS root
    ORDER BY root.watched_folder_path
"#
);

// The blank cover of the candidates reading the files that hash to
// `content_hash`.
#[cfg(test)]
coven::read!(
    writer BlankCover(content_hash: String),
    r#"
    SELECT candidate.blank_cover_file
    FROM scan_candidate AS candidate
    WHERE candidate.content_hash = :content_hash
"#
);

// The audio units of the candidates reading the files that hash to
// `content_hash`, in play order.
#[cfg(test)]
coven::read!(
    writer StoredCandidateUnits(content_hash: String) {
        slice_index: Option<u32>,
        duration_ms: u64,
        start_sample: Option<u64>,
        end_sample: Option<u64>,
        disc: Option<u32>,
        number: u32
    },
    r#"
    SELECT unit.file_id, unit.sheet_id, unit.slice_index, unit.duration_ms, unit.start_sample,
           unit.end_sample, unit.disc, unit.number
    FROM scan_candidate AS candidate
    JOIN scan_candidate_unit AS unit
      ON (unit.watched_folder_path, unit.candidate_path)
       = (candidate.watched_folder_path, candidate.path)
    WHERE candidate.content_hash = :content_hash
    ORDER BY unit.position
"#
);

// How many folders are recorded as found.
#[cfg(test)]
coven::read!(
    writer DiscoveryCount { held: i64 },
    "SELECT COUNT(*) AS held FROM folder_discovery AS found"
);
