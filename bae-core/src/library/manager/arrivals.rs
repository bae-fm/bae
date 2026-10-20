//! Files another device attached that haven't reached storage yet: a library
//! arriving on this device, kept apart from the pending work that needs
//! attention.

use super::*;
use crate::db::{ArrivingReleaseRows, AttachedFileRow, FileTable};
use std::collections::{BTreeMap, HashSet};

/// A file another device attached whose bytes storage doesn't hold yet. This
/// device read or downloaded it, storage confirmed its fixed path empty, and
/// its uploader is still active, so it waits for that device's upload (coven
/// §17.4). Nothing here can or need act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArrivingFile {
    pub uploader: coven::DeviceId,
    pub file: coven::FileId,
}

impl ArrivingFile {
    /// The file a pending record waits on its uploader for: a file's record
    /// whose first unmet condition is the object at that file's own fixed
    /// path. coven records exactly this when storage confirms the file absent
    /// while its uploader is active; a missing file, a removed uploader or a
    /// failed request is another reason, and stays a pending record.
    pub fn of(record: &coven::PendingRecord) -> Option<Self> {
        let coven::PendingSubject::File { device, file } = record.subject else {
            return None;
        };
        let coven::PendingReason::Waits(coven::Prerequisite::Object(path)) = &record.reason else {
            return None;
        };
        (path.device() == Some(device) && path.file_id() == Some(file)).then_some(Self {
            uploader: device,
            file,
        })
    }
}

/// Everything still arriving, by the device each file waits on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arrivals {
    /// Ordered by device name.
    pub devices: Vec<DeviceArrivals>,
}

/// What one device is still uploading that this device has asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceArrivals {
    pub device: coven::DeviceId,
    /// The name the device registered under.
    pub device_name: String,
    /// Ordered by album title.
    pub releases: Vec<ReleaseArrival>,
    /// Artist images and avatars.
    pub other_files: u32,
}

/// A release some of whose files are still arriving from one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseArrival {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    /// Its cover is among them.
    pub cover: bool,
    /// The tracks whose audio is among them; they can't play yet.
    pub track_ids: Vec<String>,
}

impl Arrivals {
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Each album with a release still arriving, with the device it arrives
    /// from; an album arriving from several devices names the first by name.
    pub fn albums(&self) -> HashMap<String, String> {
        let mut albums = HashMap::new();
        for device in &self.devices {
            for release in &device.releases {
                albums
                    .entry(release.album_id.clone())
                    .or_insert_with(|| device.device_name.clone());
            }
        }
        albums
    }

    /// Each release still arriving, with the device it arrives from and its
    /// arriving tracks, gathered across devices.
    pub fn releases(&self) -> HashMap<String, (String, ReleaseArrival)> {
        let mut releases: HashMap<String, (String, ReleaseArrival)> = HashMap::new();
        for device in &self.devices {
            for release in &device.releases {
                match releases.get_mut(&release.release_id) {
                    Some((_, gathered)) => {
                        gathered.cover |= release.cover;
                        gathered.track_ids.extend(release.track_ids.iter().cloned());
                    }
                    None => {
                        releases.insert(
                            release.release_id.clone(),
                            (device.device_name.clone(), release.clone()),
                        );
                    }
                }
            }
        }
        releases
    }
}

/// Which file each attached row carries, by its uploader and id. coven names
/// a pending file only by those, so the row behind one is found here. A row's
/// file is read from coven once per attachment and kept until the row changes
/// or leaves.
#[derive(Default)]
pub(crate) struct AttachedFiles {
    rows: HashMap<(FileTable, String), ([u8; 32], ArrivingFile)>,
}

impl AttachedFiles {
    /// Follow the attached rows: read the file of each new or reattached row,
    /// and forget rows that no longer carry one.
    async fn update(
        &mut self,
        database: &Database,
        attached: &[AttachedFileRow],
    ) -> Result<(), LibraryError> {
        let current: HashSet<(FileTable, &str)> = attached
            .iter()
            .map(|row| (row.table, row.key.as_str()))
            .collect();
        self.rows
            .retain(|(table, key), _| current.contains(&(*table, key.as_str())));
        for row in attached {
            let known = self.rows.get(&(row.table, row.key.clone()));
            if known.is_some_and(|(attachment, _)| *attachment == row.attachment) {
                continue;
            }
            let file = database
                .file_ref(row.table.name(), &row.key)
                .await
                .map_err(|error| {
                    LibraryError::Storage(format!(
                        "file of {} {}: {error}",
                        row.table.name(),
                        row.key
                    ))
                })?;
            self.rows.insert(
                (row.table, row.key.clone()),
                (
                    row.attachment,
                    ArrivingFile {
                        uploader: file.uploader(),
                        file: file.id(),
                    },
                ),
            );
        }
        Ok(())
    }

    /// The rows carrying these files. A file no row carries any more belongs
    /// to a row deleted since it was asked for, and arrives for nothing.
    fn rows_of(&self, files: &HashSet<ArrivingFile>) -> Vec<(FileTable, &str, ArrivingFile)> {
        let mut rows: Vec<_> = self
            .rows
            .iter()
            .filter(|(_, (_, file))| files.contains(file))
            .map(|((table, key), (_, file))| (*table, key.as_str(), *file))
            .collect();
        rows.sort_by(|a, b| (a.0.name(), a.1).cmp(&(b.0.name(), b.1)));
        let carried: HashSet<ArrivingFile> = rows.iter().map(|(_, _, file)| *file).collect();
        for file in files.difference(&carried) {
            debug!(?file, "an arriving file belongs to no row any more");
        }
        rows
    }
}

/// Group the rows of arriving files by the device each arrives from and the
/// release each belongs to.
fn group(
    rows: &[(FileTable, &str, ArrivingFile)],
    found: &ArrivingReleaseRows,
    devices: &HashMap<coven::DeviceId, String>,
) -> Result<Arrivals, LibraryError> {
    let file_release: HashMap<&str, &str> = found
        .file_releases
        .iter()
        .map(|(file, release)| (file.as_str(), release.as_str()))
        .collect();
    let mut file_tracks: HashMap<&str, Vec<&str>> = HashMap::new();
    for (file, track) in &found.file_tracks {
        file_tracks.entry(file).or_default().push(track);
    }
    let named: HashMap<&str, &crate::db::ArrivingReleaseRow> = found
        .releases
        .iter()
        .map(|release| (release.release_id.as_str(), release))
        .collect();
    let mut by_device: BTreeMap<(String, coven::DeviceId), DeviceArrivals> = BTreeMap::new();
    for (table, key, file) in rows {
        let name = devices.get(&file.uploader).ok_or_else(|| {
            LibraryError::Internal(format!(
                "file {:?} arrives from device {:?}, which no member has",
                file.file, file.uploader
            ))
        })?;
        let device = by_device
            .entry((name.clone(), file.uploader))
            .or_insert_with(|| DeviceArrivals {
                device: file.uploader,
                device_name: name.clone(),
                releases: Vec::new(),
                other_files: 0,
            });
        let release_id = match table {
            FileTable::Covers => *key,
            FileTable::ReleaseFiles => file_release.get(key).copied().ok_or_else(|| {
                LibraryError::Internal(format!("release file {key} names no release"))
            })?,
            FileTable::ArtistImages | FileTable::People => {
                device.other_files += 1;
                continue;
            }
        };
        let row = named.get(release_id).ok_or_else(|| {
            LibraryError::Internal(format!("release {release_id} names no album"))
        })?;
        let index = match device
            .releases
            .iter()
            .position(|release| release.release_id == release_id)
        {
            Some(index) => index,
            None => {
                device.releases.push(ReleaseArrival {
                    release_id: release_id.to_string(),
                    album_id: row.album_id.clone(),
                    album_title: row.album_title.clone(),
                    cover: false,
                    track_ids: Vec::new(),
                });
                device.releases.len() - 1
            }
        };
        let release = &mut device.releases[index];
        match table {
            FileTable::Covers => release.cover = true,
            FileTable::ReleaseFiles => release.track_ids.extend(
                file_tracks
                    .get(key)
                    .into_iter()
                    .flatten()
                    .map(|track| track.to_string()),
            ),
            FileTable::ArtistImages | FileTable::People => {
                unreachable!("other files are counted above")
            }
        }
    }
    let mut devices: Vec<DeviceArrivals> = by_device.into_values().collect();
    for device in &mut devices {
        device.releases.sort_by(|a, b| {
            (a.album_title.as_str(), a.release_id.as_str())
                .cmp(&(b.album_title.as_str(), b.release_id.as_str()))
        });
        for release in &mut device.releases {
            release.track_ids.sort();
            release.track_ids.dedup();
        }
    }
    Ok(Arrivals { devices })
}

/// Keeps the sync status's pending list and arrivals current: coven's pending
/// records split into the work that needs attention and the files still
/// arriving, each arriving file placed by its row and named by its device.
/// The library's attached rows are followed only while something arrives.
pub(super) struct PendingFollower {
    manager: LibraryManager,
    pending: coven::LiveQuery<Vec<coven::PendingRecord>>,
    members: coven::LiveQuery<Vec<coven::MemberInfo>>,
    attached: Option<coven::LiveQuery<Vec<AttachedFileRow>>>,
    files: AttachedFiles,
}

impl PendingFollower {
    pub(super) fn new(manager: LibraryManager) -> Self {
        Self {
            pending: manager.database.subscribe_pending(),
            members: manager.database.subscribe_members(),
            attached: None,
            files: AttachedFiles::default(),
            manager,
        }
    }

    pub(super) async fn run(mut self) {
        let mut records = None;
        let mut members = None;
        let mut attached = None;
        loop {
            tokio::select! {
                read = self.pending.next() => records = Some(read.map_err(|error| {
                    warn!(%error, "could not read pending sync records");
                    failure(error.into())
                })),
                read = self.members.next() => members = Some(read.map_err(|error| {
                    warn!(%error, "could not read the library's devices");
                    failure(error.into())
                })),
                read = next_attached(&mut self.attached) => attached = Some(read),
            }
            // The pending records are read first; a value of the others
            // before them has nothing to place yet.
            let Some(records) = &records else { continue };
            let (pending, arrivals) = match records {
                Ok(records) => self.split(records, members.as_ref(), &mut attached).await,
                Err(error) => (Err(error.clone()), Some(Err(error.clone()))),
            };
            self.manager.sync_status.set_pending(pending, arrivals);
        }
    }

    async fn split(
        &mut self,
        records: &[coven::PendingRecord],
        members: Option<&Result<Vec<coven::MemberInfo>, crate::ui::UiError>>,
        attached: &mut Option<Result<Vec<AttachedFileRow>, crate::ui::UiError>>,
    ) -> (
        Result<Vec<PendingItem>, crate::ui::UiError>,
        Option<Result<Arrivals, crate::ui::UiError>>,
    ) {
        let (arriving, attention): (Vec<_>, Vec<_>) = records
            .iter()
            .cloned()
            .partition(|record| ArrivingFile::of(record).is_some());
        let pending = self
            .manager
            .pending_items(attention)
            .await
            .map_err(|error| {
                warn!(%error, "could not name pending sync records");
                failure(error)
            });
        let arriving: HashSet<ArrivingFile> =
            arriving.iter().filter_map(ArrivingFile::of).collect();
        if arriving.is_empty() {
            self.attached = None;
            *attached = None;
            self.files = AttachedFiles::default();
            return (pending, Some(Ok(Arrivals::default())));
        }
        if self.attached.is_none() {
            let mut query = self.manager.database.subscribe_attached_file_rows();
            *attached = Some(query.next().await.map_err(|error| failure(error.into())));
            self.attached = Some(query);
        }
        let arrivals = match (members, attached.as_ref()) {
            (Some(Ok(members)), Some(Ok(attached))) => self
                .arrivals(&arriving, members, attached)
                .await
                .map_err(|error| {
                    warn!(%error, "could not place arriving files");
                    failure(error)
                }),
            (Some(Err(error)), _) | (_, Some(Err(error))) => Err(error.clone()),
            // The devices are still being read; their first value follows.
            (None, _) | (_, None) => return (pending, None),
        };
        (pending, Some(arrivals))
    }

    async fn arrivals(
        &mut self,
        arriving: &HashSet<ArrivingFile>,
        members: &[coven::MemberInfo],
        attached: &[AttachedFileRow],
    ) -> Result<Arrivals, LibraryError> {
        self.files.update(&self.manager.database, attached).await?;
        let rows = self.files.rows_of(arriving);
        let covers = rows
            .iter()
            .filter(|(table, _, _)| *table == FileTable::Covers)
            .map(|(_, key, _)| key.to_string())
            .collect();
        let files = rows
            .iter()
            .filter(|(table, _, _)| *table == FileTable::ReleaseFiles)
            .map(|(_, key, _)| key.to_string())
            .collect();
        let found = self
            .manager
            .database
            .arriving_release_rows(covers, files)
            .await?;
        let devices = members
            .iter()
            .flat_map(|member| &member.devices)
            .map(|device| (device.id, device.name.clone()))
            .collect();
        group(&rows, &found, &devices)
    }
}

fn failure(error: LibraryError) -> crate::ui::UiError {
    crate::ui::UiError::diagnostic(error.category(), error)
}

async fn next_attached(
    query: &mut Option<coven::LiveQuery<Vec<AttachedFileRow>>>,
) -> Result<Vec<AttachedFileRow>, crate::ui::UiError> {
    match query {
        Some(query) => query.next().await.map_err(|error| {
            warn!(%error, "could not read the library's attached files");
            failure(error.into())
        }),
        None => std::future::pending().await,
    }
}

#[cfg(test)]
#[path = "arrivals_tests.rs"]
mod tests;
