//! What a release's rows in this device's upload queue say of its files.

/// How many of a release's queued files only wait and how many are being
/// sent. A release has an upload only while at least one of its files is
/// queued, so the two never are both zero.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// them: those only waiting, and those this process is sending now. The
    /// read holds at least one queued file
    /// ([`release_upload_in_row`](crate::client::queued_uploads::release_upload_in_row)).
    pub fn of_release(raw: &crate::DbReleaseUpload) -> Self {
        Self { queued: raw.waiting, uploading: raw.sending }
    }

    /// Uploading while any of the files is being sent, queued while they all
    /// wait.
    pub fn activity(&self) -> UploadActivity {
        if self.uploading > 0 {
            UploadActivity::Uploading
        } else {
            UploadActivity::Queued
        }
    }
}

#[cfg(test)]
#[path = "upload_progress_tests.rs"]
mod tests;
