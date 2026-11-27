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

// The stars the person using this device gave each track they rated: the
// `stars` relation of a read that shows them their own.
sqlive_compose::fragment!(
    OwnTrackStars { track_id: Option<String> },
    r#"
    SELECT rated.track_id, rated.stars
    FROM ratings AS rated
    WHERE rated.person_id = (SELECT own.member FROM coven_members AS own WHERE own.is_self)
      AND rated.track_id IS NOT NULL
"#
);
