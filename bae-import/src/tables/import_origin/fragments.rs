//! What the import origin offers the candidate pane: the release this device imported from a candidate.

sqlive_compose::fragment!(
    ImportedRelease(key: crate::CandidateKey) { imported_key: crate::CandidateKey },
    r#"
    SELECT :key AS imported_key, release.*
    FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
    JOIN local_release_origin AS origin ON origin.content_hash = scan.content_hash
    JOIN @bae_records::records::fragments::ReleaseAlbum AS release
      ON release.release_id = origin.release_id
"#);
