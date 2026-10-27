//! What the upload queue's rows say of the files waiting in it.
pub use coven::UploadPhase as UploadState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadBar {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadActivity {
    Uploading,
    Queued,
}

/// How many queued files wait and how many are being sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadProgress {
    pub queued: u32,
    pub uploading: u32,
}
impl UploadProgress {
    pub fn activity(&self) -> Option<UploadActivity> {
        if self.uploading > 0 {
            Some(UploadActivity::Uploading)
        } else if self.queued > 0 {
            Some(UploadActivity::Queued)
        } else {
            None
        }
    }
}
