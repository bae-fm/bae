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
    /// A release's files in this device's upload queue as one read found
    /// them: those only waiting, and those this process is sending now.
    pub(crate) fn of_release(raw: &crate::db::DbReleaseUpload) -> Self {
        let count = |value: i64| u32::try_from(value).expect("a release's queued file count");
        Self {
            queued: count(raw.queued - raw.sending),
            uploading: count(raw.sending),
        }
    }

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
