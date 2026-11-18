//! The release file a failed state's pending record is about, as the status
//! query names it: the record of coven's `coven_pending` whose subject the
//! state names, joined to the release file of its file, or for a move's
//! record to the release file whose original it names.

/// The status query's column for the name of the release file the pending
/// record naming the failed state is about: for a move's record, the release
/// file of its original; for a file's, the release file of its file when the
/// file's namespace is the release files'. `NULL` when no pending record
/// names the state, or no release file is about the one that does.
pub(super) fn cause_file_column() -> String {
    use bae_records::synced::RELEASE_FILES_NAMESPACE;
    format!(
        "CASE WHEN cause.reason_kind = 'original' THEN original_file.original_filename \
         WHEN named.namespace = '{RELEASE_FILES_NAMESPACE}' THEN named_file.original_filename END"
    )
}

/// The joins [`cause_file_column`] reads, after the sync state `shown` with
/// its `cause_subject`: the pending record whose subject the state names,
/// then its file's and its original's release files, each by an index. A
/// subject names one record. More than one coven file can record the same
/// original path, so the subquery picks the release file with the smallest
/// id.
pub(super) const CAUSE_FILE_JOINS: &str = "\
     LEFT JOIN coven_pending AS cause \
         ON CAST(cause.record AS TEXT)->'$.subject' = shown.cause_subject \
     LEFT JOIN coven_files AS named ON named.id = cause.file \
     LEFT JOIN release_files AS named_file ON named_file.file = cause.file \
     LEFT JOIN release_files AS original_file ON original_file.id = ( \
         SELECT original_release_file.id \
         FROM coven_user_files AS original_user_file \
         JOIN release_files AS original_release_file \
             ON original_release_file.file = original_user_file.file \
         WHERE original_user_file.path = cause.original_path \
         ORDER BY original_release_file.id LIMIT 1)";

#[cfg(test)]
#[path = "cause_file_tests.rs"]
mod tests;
