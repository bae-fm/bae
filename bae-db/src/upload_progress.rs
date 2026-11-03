//! What a release's rows in this device's upload queue say of its files.

/// How many queued files wait and how many are being sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadProgress {
    pub queued: u32,
    pub uploading: u32,
}

/// Whether any of a release's files are being sent, or only waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadActivity {
    Uploading,
    Queued,
}

impl UploadProgress {
    /// A release's files in this device's upload queue as one read found
    /// them: those only waiting, and those this process is sending now.
    pub fn of_release(raw: &crate::DbReleaseUpload) -> Self {
        let count = |value: i64| u32::try_from(value).expect("a release's queued file count");
        Self { queued: count(raw.queued - raw.sending), uploading: count(raw.sending) }
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
