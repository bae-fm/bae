//! Files still arriving from another device, and the pictures waiting on them.

/// A file another device attached but hasn't uploaded yet, opaque to the UI,
/// which hands it back to `wait_for_arrival`. Wraps
/// `bae_core::library::ArrivingFile`.
#[derive(Debug, uniffi::Object)]
pub struct BridgeArrivingFile(pub(crate) bae_core::library::ArrivingFile);

/// An image to show: its bytes, or its file still arriving from another
/// device, which the UI waits for with `wait_for_arrival` and then reads
/// again. Arriving is no failure: the UI shows its placeholder and logs
/// nothing above debug. Mirrors `bae_core::library::ImageRead`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImageRead {
    Bytes {
        bytes: Vec<u8>,
    },
    Arriving {
        file: std::sync::Arc<BridgeArrivingFile>,
    },
}

impl BridgeImageRead {
    pub(crate) fn from_core(read: bae_core::library::ImageRead) -> Self {
        match read {
            bae_core::library::ImageRead::Bytes(bytes) => Self::Bytes { bytes },
            bae_core::library::ImageRead::Arriving(file) => Self::Arriving {
                file: std::sync::Arc::new(BridgeArrivingFile(file)),
            },
        }
    }
}

/// Everything sync is doing on this device, for the activity indicator in
/// the title bar and the list it opens. Mirrors `bae_core::library::SyncActivity`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncActivity {
    /// Bytes are moving: the indicator spins. A pass alone doesn't count.
    pub moving: bool,
    /// Something needs the person: the indicator shows a warning, and the
    /// list leads with it.
    pub needs_attention: bool,
    /// The sync failure the status line shows, such as being offline.
    pub failure: Option<super::BridgeError>,
    /// The pending records that need the person.
    pub attention: super::BridgePendingRecords,
    /// A sync pass is running.
    pub syncing: bool,
    pub uploads: Option<BridgeUploadsInFlight>,
    /// The covers and other pictures downloading as their rows arrive.
    pub artwork: Option<super::BridgeEagerCacheFillProgress>,
    pub downloads: Option<BridgeDownloadsInFlight>,
    pub arrivals: BridgeArrivals,
}

impl BridgeSyncActivity {
    pub(crate) fn from_core(activity: bae_core::library::SyncActivity) -> Self {
        let moving = activity.moving();
        let needs_attention = activity.needs_attention();
        let bae_core::library::SyncActivity {
            failure,
            attention,
            syncing,
            uploads,
            artwork,
            downloads,
            arrivals,
        } = activity;
        Self {
            moving,
            needs_attention,
            failure: failure.map(super::BridgeError::from_core),
            attention: super::BridgePendingRecords::from_core(attention),
            syncing,
            uploads: uploads.map(BridgeUploadsInFlight::from_core),
            artwork: artwork.map(super::BridgeEagerCacheFillProgress::from_core),
            downloads: downloads.map(BridgeDownloadsInFlight::from_core),
            arrivals: BridgeArrivals::from_core(arrivals),
        }
    }
}

/// The files this device is sending now, out of its whole upload queue.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeUploadsInFlight {
    pub sending: u32,
    pub waiting: u32,
    /// Provider-confirmed bytes of the files sent in parts; zero of zero when
    /// every file goes in one request.
    pub bytes_done: u64,
    pub bytes_total: u64,
}
mirror_struct! {
    BridgeUploadsInFlight = bae_core::library::UploadsInFlight,
    from_core: fn,
    fields: { sending, waiting, bytes_done, bytes_total },
}

/// The releases downloading to keep offline.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDownloadsInFlight {
    pub active: u32,
    pub queued: u32,
}
mirror_struct! {
    BridgeDownloadsInFlight = bae_core::library::DownloadsInFlight,
    from_core: fn,
    fields: { active, queued },
}

/// The files still arriving from other devices, grouped for the activity
/// list, with each album and release that has any for the library's cards
/// and album detail to look up.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeArrivals {
    Loading,
    Ready {
        /// By device name.
        devices: Vec<BridgeDeviceArrivals>,
        /// Each album with a release still arriving, by album id, with the
        /// name of the device it arrives from.
        albums: std::collections::HashMap<String, String>,
        /// Each release still arriving, by release id.
        releases: std::collections::HashMap<String, BridgeReleaseArrival>,
    },
    Failed {
        error: super::BridgeError,
    },
}

impl BridgeArrivals {
    pub(crate) fn from_core(
        arrivals: Option<Result<bae_core::library::Arrivals, bae_core::ui::UiError>>,
    ) -> Self {
        match arrivals {
            None => Self::Loading,
            Some(Err(error)) => Self::Failed {
                error: super::BridgeError::from_core(error),
            },
            Some(Ok(arrivals)) => Self::Ready {
                albums: arrivals.albums(),
                releases: arrivals
                    .releases()
                    .into_iter()
                    .map(|(id, (device_name, release))| {
                        (
                            id,
                            BridgeReleaseArrival {
                                device_name,
                                cover: release.cover,
                                track_ids: release.track_ids,
                            },
                        )
                    })
                    .collect(),
                devices: arrivals
                    .devices
                    .into_iter()
                    .map(BridgeDeviceArrivals::from_core)
                    .collect(),
            },
        }
    }
}

/// What one device is still uploading that this device asked for.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDeviceArrivals {
    pub device_name: String,
    /// By album title.
    pub releases: Vec<BridgeArrivingRelease>,
    /// Artist images and avatars.
    pub other_files: u32,
}

impl BridgeDeviceArrivals {
    fn from_core(device: bae_core::library::DeviceArrivals) -> Self {
        Self {
            device_name: device.device_name,
            releases: device
                .releases
                .into_iter()
                .map(|release| BridgeArrivingRelease {
                    release_id: release.release_id,
                    album_id: release.album_id,
                    album_title: release.album_title,
                })
                .collect(),
            other_files: device.other_files,
        }
    }
}

/// A release in the activity list, named by its album.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArrivingRelease {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
}

/// A release still arriving, as its album detail shows it: from which
/// device, whether its cover is among the files, and the tracks that can't
/// play until theirs arrive.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseArrival {
    pub device_name: String,
    pub cover: bool,
    pub track_ids: Vec<String>,
}
