//! What the import origin offers the candidate pane: the release this device imported from a candidate.

// The release this device imported from each folder hash it imported.
coven::fragment!(
    ReleaseOrigin {},
    r#"
    SELECT origin.content_hash AS origin_hash, origin.release_id AS origin_release_id
    FROM local_release_origin AS origin
"#
);

coven::fragment!(
    ImportedRelease(key: crate::CandidateKey) { imported_key: crate::CandidateKey },
    r#"
    SELECT :key AS imported_key, release.*
    FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
    JOIN @crate::tables::import_origin::fragments::ReleaseOrigin AS origin
      ON origin.origin_hash = scan.content_hash
    JOIN @bae_records::records::fragments::ReleaseAlbum AS release
      ON release.release_id = origin.origin_release_id
"#);

impl ImportedRelease {
    /// The id of the release this device imported.
    pub(crate) fn into_release_id(self) -> String {
        self.release.into_ids().0
    }
}
