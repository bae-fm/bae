//! What the people domain offers to other reads: a person's profile.

// A person's profile, which keeps its member id after the member's removal,
// with the version of its avatar's file, NULL for a profile with no avatar. An
// avatar's reference always names a record in `coven_files`, which coven
// deletes only with the last reference (coven 17.2).
sqlive_compose::fragment!(
    Profile {
        id: coven::MemberId,
        private_circle: coven::CircleId,
        history_sharing: crate::people::HistorySharing,
        avatar_id: Option<coven::FileId>,
        avatar_version: Option<String>
    },
    r#"
    SELECT p.id, p.name, p.private_circle, p.history_sharing, p.avatar_id,
           (SELECT stored.version
            FROM @bae_store::file_version::StoredFileVersion AS stored
            WHERE stored.file = p.avatar_id) AS avatar_version
    FROM people AS p
"#
);
