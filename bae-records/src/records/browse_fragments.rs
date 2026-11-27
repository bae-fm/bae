//! The rows the library views' artist, composer and work reads join to what
//! their own tables list: an artist, a work as its rows show it, a work's
//! children, a release that performs a work, and a composer's unlinked
//! credits with their tracks and albums.

use bae_catalogs::catalog::Catalog;
use bae_store::DbError;
use bae_types::credit_role::CreditedAs;
use bae_types::pressing::MediaCount;
use time::UtcDateTime;

use crate::credits::CreditName;
use crate::{
    DbAlbum, DbArtist, DbReleaseArtistRole, DbTrack, DbTrackArtistRole, DbTrackWork, DbWork,
};

sqlive_compose::fragment!(
    ArtistRecord,
    r#"
    SELECT artist.id AS artist_id, artist.name, artist.sort_name, artist.discogs_artist_id,
           artist.musicbrainz_artist_id, artist.created_at
    FROM artists AS artist
"#
);

impl ArtistRecord {
    /// The artist.
    pub fn into_artist(self) -> Result<DbArtist, DbError> {
        let created_at = timestamp("an artist's created_at", &self.created_at)?;
        Ok(DbArtist {
            id: self.artist_id,
            name: self.name,
            sort_name: self.sort_name,
            discogs_artist_id: self.discogs_artist_id,
            musicbrainz_artist_id: self.musicbrainz_artist_id,
            created_at,
        })
    }
}

// Each performance of a work, one for each track that performs it, with the
// values that order the releases performing a work: by album title, compared by
// the title's sort key in the device's language, then album id, so two albums
// sharing a title don't interleave, then the album's own order of releases,
// then release id. A release with several such tracks has a row for each:
// a `DISTINCT` here would keep SQLite from reading one work's rows through
// that work where a read names it by a row's column.
sqlive_compose::fragment!(
    WorkPerformances { title_key: Vec<u8> },
    r#"
    SELECT performed.id AS link_id, performed.work_id, release.id AS release_id,
           bae_name_key(album.title, (
               SELECT sort.language
               FROM @crate::records::name_sort::NameSortLanguage AS sort)) AS title_key,
           album.id AS album_id, release.album_order
    FROM track_works AS performed
    JOIN tracks AS track ON track.id = performed.track_id
    JOIN releases AS release ON release.id = track.release_id
    JOIN albums AS album ON album.id = release.album_id
"#
);

// Each work as a row shows it: its own fields, its composers' names in credit
// order, how many releases perform it, and the release whose cover it shows.
// That release is the first of those performing it in the order of
// [`WorkPerformances`]: the first album's primary release when it
// performs the work, otherwise its first release in album order that does.
sqlive_compose::fragment!(
    WorkSummary {
        linked_release_count: i64,
        representative_release_id: Option<String>,
    },
    r#"
    SELECT work.id AS work_id, work.title, work.disambiguation, work.work_type,
           work.musicbrainz_work_id, work.created_at,
           @crate::records::credit_fragments::WorkComposerNames(parent: work) AS composers,
           (SELECT COUNT(DISTINCT performing.release_id)
            FROM track_works AS linked
            JOIN tracks AS performing ON performing.id = linked.track_id
            WHERE linked.work_id = work.id) AS linked_release_count,
           (SELECT shown.release_id
            FROM @crate::records::browse_fragments::WorkPerformances AS shown
            WHERE shown.work_id = work.id
            ORDER BY shown.title_key, shown.album_id, shown.album_order, shown.release_id
            LIMIT 1) AS representative_release_id
    FROM works AS work
"#
);

/// A work as its rows show it, as [`WorkSummary`] reads it.
pub struct SummarizedWork {
    pub work: DbWork,
    /// The composers' names joined in credit order; `None` for a work with
    /// none.
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
    /// The release whose cover the work shows; `None` when no release
    /// performs it.
    pub representative_release_id: Option<String>,
}

impl WorkSummary {
    /// The work and what its rows show of it.
    pub fn into_summarized(self) -> Result<SummarizedWork, DbError> {
        let created_at = timestamp("a work's created_at", &self.created_at)?;
        Ok(SummarizedWork {
            work: DbWork {
                id: self.work_id,
                title: self.title,
                disambiguation: self.disambiguation,
                work_type: self.work_type,
                musicbrainz_work_id: self.musicbrainz_work_id,
                created_at,
            },
            composer_names: self.composers.into_names(),
            linked_release_count: self.linked_release_count,
            representative_release_id: self.representative_release_id,
        })
    }
}

// The children of the work `work`, one row each, with the values their order
// reads: its parts (`section` 0), by their place among the work's parts, then
// by title; then the releases performing it (`section` 1), in the order of
// [`WorkPerformances`]. Titles compare by their sort keys in the
// device's language. `item_id` is the part's work or the release.
sqlive_compose::fragment!(
    WorkChildren(work: String) {
        section: i64,
        place: Option<i64>,
        title_key: Vec<u8>,
        album_id: Option<String>,
        album_order: Option<f64>,
        item_id: String,
    },
    r#"
    SELECT 0 AS section, part.position AS place,
           bae_name_key(child.title, (
               SELECT sort.language
               FROM @crate::records::name_sort::NameSortLanguage AS sort)) AS title_key,
           NULL AS album_id, NULL AS album_order, part.child_work_id AS item_id
    FROM work_parts AS part
    JOIN works AS child ON child.id = part.child_work_id
    WHERE part.parent_work_id = :work
    UNION ALL
    SELECT DISTINCT 1, NULL, shown.title_key, shown.album_id, shown.album_order, shown.release_id
    FROM @crate::records::browse_fragments::WorkPerformances AS shown
    WHERE shown.work_id = :work
"#
);

impl WorkChildren {
    /// Whether the child is a part of the work, and otherwise a release that
    /// performs it.
    pub fn is_part(&self) -> Result<bool, DbError> {
        match self.section {
            0 => Ok(true),
            1 => Ok(false),
            other => Err(DbError::unreadable("a work child's section", other)),
        }
    }
}

// Each release as the list of a work's releases shows it: its album, its name,
// year and media, and its place among its album's releases.
sqlive_compose::fragment!(
    WorkReleaseRow {
        album_title: Option<String>,
        release_number: Option<i64>,
    },
    r#"
    SELECT release.id AS release_id, release.album_id,
           (SELECT album.title FROM albums AS album
            WHERE album.id = release.album_id) AS album_title,
           release.release_name, release.year,
           @crate::records::pressing_columns::ReleaseMedia(parent: release) AS media,
           (SELECT numbered.number FROM release_number_in_album AS numbered
            WHERE numbered.release_id = release.id) AS release_number
    FROM releases AS release
"#
);

/// A release that performs a work, as [`WorkReleaseRow`] reads it.
pub struct PerformingRelease {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub media: Vec<MediaCount>,
    pub release_number: i64,
}

impl WorkReleaseRow {
    /// The release.
    pub fn into_release(self) -> Result<PerformingRelease, DbError> {
        let missing = |what: &str| {
            DbError::unreadable(
                format!("the {what} of release {}", self.release_id),
                None::<String>,
            )
        };
        let media = self.media.counts()?;
        let album_title = self.album_title.clone().ok_or_else(|| missing("album"))?;
        let release_number = self.release_number.ok_or_else(|| missing("number"))?;
        let year = self
            .year
            .map(|year| {
                i32::try_from(year).map_err(|_error| DbError::unreadable("a release's year", year))
            })
            .transpose()?;
        Ok(PerformingRelease {
            media,
            year,
            release_id: self.release_id,
            album_id: self.album_id,
            album_title,
            release_name: self.release_name,
            release_number,
        })
    }
}

// A release's catalog role credit, with the other printed name it names, if
// any.
sqlive_compose::fragment!(
    ReleaseRoleCredit {
        position: i32,
        credit_name_key: Option<String>,
        credit_name: Option<String>,
    },
    r#"
    SELECT role.id AS role_id, role.release_id, role.artist_id, role.position, role.source,
           role.role, role.role_words, role.qualifiers, role.created_at,
           (SELECT printed.name_key FROM artist_credits AS printed
            WHERE printed.artist_id = role.artist_id
              AND printed.name_key = role.credit_key) AS credit_name_key,
           (SELECT printed.name FROM artist_credits AS printed
            WHERE printed.artist_id = role.artist_id
              AND printed.name_key = role.credit_key) AS credit_name
    FROM release_artist_roles AS role
"#
);

impl ReleaseRoleCredit {
    /// The credit.
    pub fn into_role(self) -> Result<DbReleaseArtistRole, DbError> {
        let stored = StoredRole {
            id: &self.role_id,
            source: &self.source,
            role: &self.role,
            role_words: self.role_words,
            qualifiers: &self.qualifiers,
            credit_name_key: self.credit_name_key,
            credit_name: self.credit_name,
            created_at: &self.created_at,
        };
        let ReadRole { source, role, credit, created_at } = stored.read()?;
        Ok(DbReleaseArtistRole {
            id: self.role_id,
            release_id: self.release_id,
            artist_id: self.artist_id,
            position: self.position,
            source,
            role,
            credit,
            created_at,
        })
    }
}

// A track's catalog role credit, with the other printed name it names, if any.
sqlive_compose::fragment!(
    TrackRoleCredit {
        position: i32,
        credit_name_key: Option<String>,
        credit_name: Option<String>,
    },
    r#"
    SELECT role.id AS role_id, role.track_id, role.artist_id, role.position, role.source,
           role.role, role.role_words, role.qualifiers, role.created_at,
           (SELECT printed.name_key FROM artist_credits AS printed
            WHERE printed.artist_id = role.artist_id
              AND printed.name_key = role.credit_key) AS credit_name_key,
           (SELECT printed.name FROM artist_credits AS printed
            WHERE printed.artist_id = role.artist_id
              AND printed.name_key = role.credit_key) AS credit_name
    FROM track_artist_roles AS role
"#
);

impl TrackRoleCredit {
    /// The credit.
    pub fn into_role(self) -> Result<DbTrackArtistRole, DbError> {
        let stored = StoredRole {
            id: &self.role_id,
            source: &self.source,
            role: &self.role,
            role_words: self.role_words,
            qualifiers: &self.qualifiers,
            credit_name_key: self.credit_name_key,
            credit_name: self.credit_name,
            created_at: &self.created_at,
        };
        let ReadRole { source, role, credit, created_at } = stored.read()?;
        Ok(DbTrackArtistRole {
            id: self.role_id,
            track_id: self.track_id,
            artist_id: self.artist_id,
            position: self.position,
            source,
            role,
            credit,
            created_at,
        })
    }
}

/// The columns a release's and a track's role credit store alike.
struct StoredRole<'a> {
    id: &'a str,
    source: &'a str,
    role: &'a str,
    role_words: Option<String>,
    qualifiers: &'a str,
    credit_name_key: Option<String>,
    credit_name: Option<String>,
    created_at: &'a str,
}

/// What [`StoredRole`]'s columns say.
struct ReadRole {
    source: Catalog,
    role: CreditedAs,
    credit: Option<CreditName>,
    created_at: UtcDateTime,
}

impl StoredRole<'_> {
    fn read(self) -> Result<ReadRole, DbError> {
        let unreadable = |what: &str, value: String| {
            DbError::unreadable(format!("role credit {} {what}", self.id), value)
        };
        let credit = super::read::joined_credit_name(self.credit_name_key, self.credit_name)
            .map_err(|error| unreadable("credit name", error))?;
        Ok(ReadRole {
            source: self
                .source
                .parse()
                .map_err(|_error| unreadable("source", self.source.into()))?,
            role: CreditedAs::from_columns(self.role, self.role_words, self.qualifiers)
                .map_err(|error| unreadable("role", error.to_string()))?,
            credit,
            created_at: timestamp("a role credit's created_at", self.created_at)?,
        })
    }
}

sqlive_compose::fragment!(
    TrackRecord {
        side: std::num::NonZeroU8,
        track_number: std::num::NonZeroU32,
        duration_ms: u64,
        stated_length_ms: Option<u64>,
    },
    r#"
    SELECT track.id AS track_id, track.release_id, track.title, track.medium_id, track.side,
           track.track_number, track.duration_ms, track.stated_length_ms, track.created_at
    FROM tracks AS track
"#
);

impl TrackRecord {
    /// The track.
    pub fn into_track(self) -> Result<DbTrack, DbError> {
        let created_at = timestamp("a track's created_at", &self.created_at)?;
        Ok(DbTrack {
            id: self.track_id,
            release_id: self.release_id,
            title: self.title,
            medium_id: self.medium_id,
            side: self.side,
            track_number: self.track_number,
            duration_ms: self.duration_ms,
            stated_length_ms: self.stated_length_ms,
            created_at,
        })
    }
}

sqlive_compose::fragment!(
    AlbumRecord { year: Option<i32>, is_compilation: bool },
    r#"
    SELECT album.id AS album_id, album.title, album.year, album.is_compilation,
           album.created_at
    FROM albums AS album
"#
);

impl AlbumRecord {
    /// The album.
    pub fn into_album(self) -> Result<DbAlbum, DbError> {
        let created_at = timestamp("an album's created_at", &self.created_at)?;
        Ok(DbAlbum {
            id: self.album_id,
            title: self.title,
            year: self.year,
            is_compilation: self.is_compilation,
            created_at,
        })
    }
}

// The artist each artist shows as after merges, itself unless it was merged.
sqlive_compose::fragment!(
    ShownArtist,
    r#"
    SELECT shown.artist_id, shown.shown_artist_id
    FROM artist_shown_as AS shown
"#
);

// Each track's link to a work it performs, with the track's place in its
// release's playing order.
sqlive_compose::fragment!(
    TrackWorkLink { position: i32 },
    r#"
    SELECT link.id AS link_id, link.track_id, link.work_id, link.position, link.source,
           link.created_at, track.position AS track_position
    FROM track_works AS link
    JOIN tracks AS track ON track.id = link.track_id
"#
);

impl TrackWorkLink {
    /// The link.
    pub fn into_link(self) -> Result<DbTrackWork, DbError> {
        let source = self.source.parse().map_err(|_error| {
            DbError::unreadable(format!("track work link {} source", self.link_id), &self.source)
        })?;
        let created_at = timestamp("a track work link's created_at", &self.created_at)?;
        Ok(DbTrackWork {
            id: self.link_id,
            track_id: self.track_id,
            work_id: self.work_id,
            position: self.position,
            source,
            created_at,
        })
    }
}

/// The time the text `value` of the column `column` names.
fn timestamp(column: &'static str, value: &str) -> Result<UtcDateTime, DbError> {
    bae_util::timestamp::parse_rfc3339(value)
        .map_err(|_unparsed| DbError::unreadable(column, value))
}
