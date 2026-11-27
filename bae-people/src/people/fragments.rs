//! What the people domain offers to other reads: a person's profile.

// A person's profile, which keeps its member id after the member's removal,
// with its avatar's stored file, none for a profile with no avatar. An
// avatar's reference always names a record in `coven_files`, which coven
// deletes only with the last reference (coven 17.2).
sqlive_compose::fragment!(
    Profile {
        id: coven::MemberId,
        private_circle: coven::CircleId,
        history_sharing: crate::people::HistorySharing
    },
    r#"
    SELECT p.id, p.name, p.private_circle, p.history_sharing, avatar.*
    FROM people AS p
    LEFT JOIN @bae_store::file_version::StoredFileVersion AS avatar ON avatar.file = p.avatar_id
"#
);
