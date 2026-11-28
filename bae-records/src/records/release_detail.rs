//! A release as its detail reads it: its tracks by side with their credits
//! and works, its credits by role, its files with the lists among them, its
//! records, the other device its audio is on, and whether it is pinned,
//! stored, or queued to upload. [`ReleaseDetail`] is the one statement of it,
//! which every read of a release's detail spreads.

use super::read::*;
use super::*;

// One release's detail, a row for each release: its row's columns, then its
// labels, barcodes, media, Discogs details, tracks in groups (each with the
// stars `stars` gives it), credits, files, records, storage row, cover and
// number in its album.
coven::fragment!(
    ReleaseDetail
    with stars(track_id TEXT, stars INTEGER) {
        release_number: Option<i64>
    },
    r#"
    SELECT rel.id AS detail_release_id, rel.album_order, held.*,
           @crate::records::track_group_fragments::TotalDuration(parent: rel)
               AS total_duration,
           @crate::records::track_group_fragments::TrackGroups(
               parent: rel, stars: (SELECT * FROM stars)) AS groups,
           @crate::records::credit_fragments::ReleaseCredits(parent: rel) AS credits,
           @crate::records::credit_fragments::ReleaseHasCredits(parent: rel) AS credit_presence,
           @crate::records::track_group_fragments::PerformsWorks(parent: rel) AS work_presence,
           @crate::records::file_fragments::ReleaseFiles(
               parent: rel,
               files: (SELECT file.*
                       FROM @crate::records::file_fragments::ReleaseFileColumns AS file
                       WHERE file.release_id = rel.id),
               segments: (SELECT segment.file_id
                          FROM release_files AS heard
                          JOIN audio_format_segments AS segment ON segment.file_id = heard.id
                          WHERE heard.release_id = rel.id),
               carved: (SELECT cut.sheet_file_id
                        FROM release_files AS carving
                        JOIN carved_tracks AS cut ON cut.sheet_file_id = carving.id
                        WHERE carving.release_id = rel.id),
               sheets: (SELECT NULL, NULL WHERE 0)) AS file_lists,
           described.*,
           storage.*,
           cover.*,
           picture.source_file AS cover_source_file,
           picture.embedded_file AS cover_embedded_file,
           (SELECT numbered.number FROM release_number_in_album AS numbered
            WHERE numbered.release_id = rel.id) AS release_number
    FROM releases AS rel
    JOIN @crate::records::read::ReleaseColumns AS held ON held.columns_release_id = rel.id
    JOIN @crate::records::fragments::ReleaseRecords AS described
      ON described.records_release_id = rel.id
    JOIN @crate::release_storage::ReleaseStorage AS storage ON storage.release_id = rel.id
    LEFT JOIN covers AS picture ON picture.id = rel.id
    LEFT JOIN @crate::records::files::CoverVersion AS cover ON cover.id = picture.id
"#
);

impl ReleaseDetail {
    /// The release's detail, one entry per track. A value no column of the
    /// library may hold fails.
    pub fn decoded(self) -> Result<DbReleaseDetail, DbError> {
        let release = self.held.release()?;
        let total = self.total_duration.total_duration_ms;
        let number = self
            .release_number
            .ok_or_else(|| DbError::unreadable("a release's number in its album", &release.id))?;
        let cover = self.cover.and_then(|cover| cover.version).map(|version| DbReleaseCover {
            version,
            source: CoverSource::of_columns(self.cover_source_file, self.cover_embedded_file),
        });
        let upload = self.storage.upload()?;
        let elsewhere = elsewhere_of_json(self.storage.elsewhere())?;
        let (pinned, remote) = (self.storage.pinned(), self.storage.stored()?);
        Ok(DbReleaseDetail {
            total_duration_ms: u64::try_from(total)
                .map_err(|_error| DbError::unreadable("a release's total length", total))?,
            track_groups: super::release_tracks::track_groups_of_json(&self.groups.track_groups)?,
            credits: super::release_credits::credit_rows_of_json(&self.credits.credit_rows)?,
            has_credits: self.credit_presence.has_credits,
            performs_works: self.work_presence.performs_works,
            files: release_files_of_json(&self.file_lists.release_files)?,
            has_records: self.described.records_kind().is_some(),
            records: super::read::records_of_json(self.described.records_json())?,
            elsewhere,
            pinned,
            remote,
            upload,
            cover,
            release_number: number,
            release,
        })
    }
}

// The title of the album a release is in, with its year, its credits and
// whether it is a compilation.
coven::fragment!(
    ReleaseAlbumFacts { year: Option<i64>, is_compilation: bool, album_created_at: String },
    r#"
    SELECT album.id AS album_id, album.year, album.is_compilation,
           album.created_at AS album_created_at
    FROM albums AS album
"#
);

impl ReleaseAlbumFacts {
    /// Whether the album is a compilation.
    pub fn is_compilation(&self) -> bool {
        self.is_compilation
    }

    /// When the album was made.
    pub fn created_at(&self) -> Result<time::UtcDateTime, DbError> {
        bae_util::timestamp::parse_rfc3339(&self.album_created_at).map_err(|_unparsed| {
            DbError::unreadable("an album's created_at", &self.album_created_at)
        })
    }

    /// The album's year, if it states one.
    pub fn year(&self) -> Result<Option<i32>, DbError> {
        album_year(self.year)
    }
}

/// The release editor's Files rows and their CUE placement choices, from an
/// [`EditorFiles`](super::editor_files::EditorFiles) row.
#[cfg(desktop)]
pub fn release_files_of(
    listed: super::editor_files::EditorFiles,
) -> Result<DbReleaseFiles, DbError> {
    release_files_of_json(&listed.files.release_files)
}

/// The release editor's Files rows and their CUE placement choices, read
/// once.
#[cfg(all(desktop, test))]
pub fn release_files_on(
    sql: &impl bae_store::WriterRead,
    form: super::editor_files::EditorReleaseFilesParams,
) -> Result<DbReleaseFiles, DbError> {
    release_files_of(bae_store::statement_row(sql, &form)?.listed)
}

/// One entry of the `carried` list of
/// [`ReleaseFiles`](super::file_fragments::ReleaseFiles).
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum CarriedColumns {
    File(FileColumns),
    Sheet(super::sheet_files::SheetWithoutTracksColumns),
}

/// The release's files as [`ReleaseFiles`](super::file_fragments::ReleaseFiles)
/// wrote them into its document `json`.
fn release_files_of_json(json: &str) -> Result<DbReleaseFiles, DbError> {
    #[derive(serde::Deserialize)]
    struct Stored {
        files: Vec<FileColumns>,
        images: Vec<FileColumns>,
        others: Vec<FileColumns>,
        carried: Vec<CarriedColumns>,
        has_others: i64,
        count: i64,
        image_count: u32,
        total_size: u64,
        #[serde(deserialize_with = "crate::source_audio::SourceAudioSummary::deserialize_stored")]
        source_audio: Option<crate::source_audio::SourceAudioSummary>,
    }
    const DOCUMENT: &str = "a release's files";
    let stored: Stored = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json(DOCUMENT, json, error))?;
    let file = |columns: FileColumns| {
        file_of(columns)
            .map_err(|(field, reason)| DbError::unreadable(DOCUMENT, format!("{field}: {reason}")))
    };
    let carried = stored
        .carried
        .into_iter()
        .map(|carried| match carried {
            CarriedColumns::File(columns) => file(columns).map(DbCarriedFile::File),
            CarriedColumns::Sheet(sheet) => sheet
                .decoded()
                .map(DbCarriedFile::Sheet)
                .map_err(|reason| DbError::unreadable(DOCUMENT, reason)),
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let files = |columns: Vec<FileColumns>| {
        columns.into_iter().map(file).collect::<Result<Vec<_>, DbError>>()
    };
    Ok(DbReleaseFiles {
        files: files(stored.files)?,
        images: files(stored.images)?,
        others: files(stored.others)?,
        carried,
        has_others: stored.has_others != 0,
        count: stored.count,
        image_count: stored.image_count,
        total_size: stored.total_size,
        source_audio: stored.source_audio,
    })
}

/// The other device a release's audio is on, from the `elsewhere` text of its
/// storage row ([`crate::release_storage::ReleaseStorage`]); `None` when no
/// other device alone holds an audio file of the release. A device that coven
/// doesn't register, or that is this device, fails the read: the release's
/// storage row names only a device that coven's record of a file names, and
/// never this one, so either is a bug.
fn elsewhere_of_json(json: &str) -> Result<Option<OtherDevice>, DbError> {
    type Registration = (String, String, i64, i64);
    const COLUMN: &str = "a release's other device";
    let Some((device, registration)) =
        serde_json::from_str::<Option<(String, Option<Registration>)>>(json)
            .map_err(|error| DbError::unreadable_json(COLUMN, json, error))?
    else {
        return Ok(None);
    };
    let fail = |cause: String| DbError::unreadable(COLUMN, cause);
    let Some((name, state, member_removed, is_self)) = registration else {
        return Err(fail(format!(
            "the release's storage row names the device {device}, which coven doesn't register"
        )));
    };
    if is_self != 0 {
        return Err(fail(format!(
            "the release's storage row names this device, {device}, as the other device \
             that alone holds one of its audio files"
        )));
    }
    let state = state.parse().map_err(|error| fail(format!("{error}")))?;
    Ok(Some(other_device(name, state, member_removed != 0)))
}

/// Why bae could not name the device that a failed read of a release file
/// said holds the file.
#[derive(Debug, thiserror::Error)]
pub enum OtherDeviceError {
    /// The read named this device as the only one that holds the file. coven
    /// keeps the bytes of this device's files here while their record exists
    /// (coven 17.3), so a read that names this device is a bug in coven.
    #[error("the read named this device, {0:?}, as the only one that holds the file")]
    ThisDevice(coven::DeviceId),
    #[error(transparent)]
    Database(#[from] DbError),
}

// The device `device` as coven registers it: its name, its state, whether a
// member removed its person from the library, and whether it is this device.
coven::read!(
    writer OtherDeviceRegistration(device: coven::DeviceId) {
        state: coven::DeviceState,
        removed: bool,
        is_self: bool,
    },
    r#"
    SELECT registered.name, registered.state, member.removed, registered.is_self
    FROM coven_devices AS registered
    JOIN coven_members AS member ON member.member = registered.member
    WHERE registered.device = :device
"#
);

impl Records {
    /// The device `device` that a read of a release file named as the only
    /// one that holds the file (`coven::FileReadError::DeviceOnly`), with its
    /// name and state from coven's registration of it in `coven_devices`.
    pub async fn other_device(
        &self,
        device: coven::DeviceId,
    ) -> Result<OtherDevice, OtherDeviceError> {
        let OtherDeviceRegistrationRow { name, state, removed: member_removed, is_self } =
            self.store.read_row(OtherDeviceRegistrationParams { device }).await?;
        if is_self {
            return Err(OtherDeviceError::ThisDevice(device));
        }
        Ok(other_device(name, state, member_removed))
    }
}

/// Another device named `name`, as coven's registration `state` of it reads:
/// removed when its registration says so or its person was removed from the
/// library (`member_removed`), replaced when it was reset.
fn other_device(name: String, state: coven::DeviceState, member_removed: bool) -> OtherDevice {
    match OtherDevice::of(name, state) {
        OtherDevice::Active { name } if member_removed => {
            OtherDevice::Former { name, state: bae_store::FormerDeviceState::Removed }
        }
        device => device,
    }
}

#[cfg(test)]
#[path = "release_detail_tests.rs"]
mod tests;
