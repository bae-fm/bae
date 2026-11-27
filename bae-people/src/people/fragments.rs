//! What the people domain offers to other reads: a person's profile, the
//! stars the person using this device gave tracks, and the plays History
//! lists.

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

impl Profile {
    /// The person as a row beside their work shows them.
    pub fn into_person_ref(self) -> crate::people::PersonRef {
        crate::people::PersonRef {
            person: self.id,
            name: self.name,
            avatar_version: self.avatar.map(|avatar| avatar.into_parts().1),
        }
    }
}

// Each play's place in History's order, newest first (when it began, then its
// id), and whose it is. `history_order` and `history_person_order` index that
// order over everyone's plays and over one person's.
sqlive_compose::fragment!(
    PlayPlace,
    r#"
    SELECT play.started_at, play.id, play.person_id
    FROM history AS play
"#
);

// Each play as History of everyone's plays lists it. `opens_day`: it is the
// newest play of its day, which `history_day` finds.
sqlive_compose::fragment!(
    ListedPlay { opens_day: bool },
    r#"
    SELECT play.started_at, play.id, play.person_id, play.track_id, play.played_on,
           play.id = (SELECT newest.id FROM history AS newest
                      WHERE newest.played_on = play.played_on
                      ORDER BY newest.started_at DESC, newest.id DESC
                      LIMIT 1) AS opens_day
    FROM history AS play
"#
);

// Each play as History of one person's plays lists it. `opens_day`: it is that
// person's newest play of its day, which `history_person_day` finds.
sqlive_compose::fragment!(
    ListedPersonPlay { opens_day: bool },
    r#"
    SELECT play.started_at, play.id, play.person_id, play.track_id, play.played_on,
           play.id = (SELECT newest.id FROM history AS newest
                      WHERE newest.person_id = play.person_id
                        AND newest.played_on = play.played_on
                      ORDER BY newest.started_at DESC, newest.id DESC
                      LIMIT 1) AS opens_day
    FROM history AS play
"#
);

// Whether any of the tracks `asked` has what a person keeps of it: a rating,
// a play in the history or a playlist entry. Deleting the track deletes them.
sqlive_compose::fragment!(
    TracksHaveKeptData with asked(track_id TEXT) { kept: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM asked
                   JOIN ratings AS rated ON rated.track_id = asked.track_id)
        OR EXISTS (SELECT 1 FROM asked
                   JOIN history AS played ON played.track_id = asked.track_id)
        OR EXISTS (SELECT 1 FROM asked
                   JOIN playlist_entries AS entry ON entry.track_id = asked.track_id) AS kept
"#
);

impl TracksHaveKeptData {
    /// Whether any of the tracks has a rating, a play or a playlist entry.
    pub fn any(&self) -> bool {
        self.kept
    }
}
