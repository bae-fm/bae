//! What the upload queue's rows say of the files being sent.
pub use coven::UploadPhase as UploadState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadBar {
    pub bytes_done: u64,
    pub bytes_total: u64,
}
