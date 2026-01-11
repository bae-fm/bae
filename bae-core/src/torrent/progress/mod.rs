pub mod handle;
use crate::import::FolderMetadata;
pub use handle::TorrentProgressHandle;
#[derive(Debug, Clone)]
pub enum TorrentProgress {
    WaitingForMetadata {
        info_hash: String,
    },
    TorrentInfoReady {
        info_hash: String,
        name: String,
        total_size: u64,
        num_files: usize,
    },
    StatusUpdate {
        info_hash: String,
        num_peers: i32,
        num_seeds: i32,
        trackers: Vec<TrackerStatus>,
    },
    MetadataFilesDetected {
        info_hash: String,
        files: Vec<String>,
    },
    MetadataProgress {
        info_hash: String,
        file: String,
        progress: f32,
    },
    MetadataComplete {
        info_hash: String,
        detected: Option<FolderMetadata>,
    },
    Error {
        info_hash: String,
        message: String,
    },
}
#[derive(Debug, Clone)]
pub struct TrackerStatus {
    pub url: String,
    pub status: String,
    pub message: Option<String>,
}
