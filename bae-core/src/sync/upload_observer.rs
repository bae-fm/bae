//! bae's `BlobTransitionObserver` — UI bookkeeping only.
//!
//! coven owns the whole blob lifecycle: the upload drain, the make-Remote gate
//! flip + local ownership cleanup, and the make-Local materialize + retract.
//! User-provided source files remain untouched; coven drops their external
//! references once the release is Remote. This observer only *reports* what
//! coven did, so the UI stays current:
//!
//! - preparation and upload callbacks record what moved into the shared
//!   [`LiveUploads`], which wakes the sync controller's outbox projection;
//! - durable queue changes, including terminal publication, arrive through
//!   coven's cloud-outbox live query rather than lifecycle callbacks.
//!
//! coven awaits each callback inside its upload, so a callback only records
//! its fact and returns: it neither reads the database nor waits for the
//! projection to publish.
//!
//! The pause state `LiveUploads` carries lets coven suspend active preparation
//! and provider futures without touching the durable queue or discarding open
//! upload sessions.
use crate::library::live_uploads::LiveUploads;

/// Records coven's blob transitions into the live upload state the outbox
/// snapshot reports while a make-Remote upload runs.
pub(crate) struct ReleaseUploadObserver {
    uploads: LiveUploads,
}

impl ReleaseUploadObserver {
    pub(crate) fn new(uploads: LiveUploads) -> Self {
        Self { uploads }
    }
}

#[async_trait::async_trait]
impl coven::BlobTransitionObserver for ReleaseUploadObserver {
    async fn on_blob_preparation_started(&self, upload: &coven::RowBlobRef) {
        self.uploads.preparation_started(upload);
    }

    async fn on_blob_preparation_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        self.uploads
            .preparation_progress(upload, bytes_done, bytes_total);
    }

    async fn on_blob_upload_started(&self, upload: &coven::RowBlobRef) {
        self.uploads.upload_started(upload);
    }

    async fn on_blob_upload_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        self.uploads
            .upload_progress(upload, bytes_done, bytes_total);
    }

    async fn on_blob_uploaded(&self, upload: &coven::RowBlobRef) {
        self.uploads.upload_finished(upload);
    }

    async fn on_blob_upload_failed(&self, upload: &coven::RowBlobRef, _error: &str) {
        // coven's drain records the attempt count and the error on its own
        // queue entry; the durable outbox reports them.
        self.uploads.upload_ended(upload);
    }

    fn on_blob_upload_abandoned(&self, upload: &coven::RowBlobRef) {
        self.uploads.upload_ended(upload);
    }

    fn should_skip_uploads(&self) -> bool {
        self.uploads.is_paused()
    }

    async fn wait_until_uploads_paused(&self) {
        self.uploads.wait_until_paused().await;
    }

    async fn wait_until_uploads_resumed(&self) {
        self.uploads.wait_until_resumed().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::outbox_snapshot::TransientUploadState;
    use crate::library::ByteProgress;
    use coven::BlobTransitionObserver;
    use std::sync::Arc;

    fn observer() -> (ReleaseUploadObserver, LiveUploads) {
        let uploads = LiveUploads::new();
        (ReleaseUploadObserver::new(uploads.clone()), uploads)
    }

    fn test_blob() -> coven::RowBlobRef {
        coven::RowBlobRef::new(
            crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
            "00415c7f-b363-4ed9-8aad-422b93e974e9".to_string(),
            "0000000001000-0000-device-a".to_string(),
            "blob_id".to_string(),
            coven::BlobRef {
                namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
                id: "357d9eb4-a021-4555-8713-0bc652d83c65".to_string(),
                scope: coven::BlobScope::Master,
                cloud_path: None,
                provenance: coven::Provenance::HostProvided,
                fill: coven::CacheFill::CacheEager,
            },
            1000,
            coven::ObjectHash::digest(b"upload-observer-test"),
            coven::RowBlobAuthority::Local,
            None,
        )
        .expect("valid observer test blob")
    }

    #[tokio::test]
    async fn pause_waiters_wake_on_each_absolute_state_change() {
        let (observer, uploads) = observer();
        let observer = Arc::new(observer);

        let waiting_for_pause = tokio::spawn({
            let observer = observer.clone();
            async move { observer.wait_until_uploads_paused().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiting_for_pause.is_finished());
        uploads.set_paused(true);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting_for_pause)
            .await
            .expect("pause notification")
            .expect("pause waiter task");

        let waiting_for_resume = tokio::spawn({
            let observer = observer.clone();
            async move { observer.wait_until_uploads_resumed().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiting_for_resume.is_finished());
        uploads.set_paused(false);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting_for_resume)
            .await
            .expect("resume notification")
            .expect("resume waiter task");
    }

    /// A second start begins a new attempt; the first one's measurement is
    /// dropped rather than left to collide with it.
    #[tokio::test]
    async fn a_second_provider_start_begins_a_new_attempt() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 600, 1016).await;
        observer.on_blob_upload_started(&blob).await;

        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(TransientUploadState::UploadStarted)
        );
        observer.on_blob_upload_progress(&blob, 100, 1016).await;
        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(TransientUploadState::Uploading(
                ByteProgress::new(100, 1016).unwrap()
            ))
        );
    }

    /// A preparation start with the previous attempt still recorded begins a
    /// new attempt too.
    #[tokio::test]
    async fn a_second_preparation_start_begins_a_new_attempt() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_preparation_started(&blob).await;
        observer
            .on_blob_preparation_progress(&blob, 500, 1000)
            .await;
        observer.on_blob_preparation_started(&blob).await;

        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(TransientUploadState::Preparing(ByteProgress::none_of(1000)))
        );
    }

    #[tokio::test]
    async fn provider_callbacks_advance_exact_transient_bytes_and_end_throughput() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 600, 1016).await;
        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(TransientUploadState::Uploading(
                ByteProgress::new(600, 1016).unwrap()
            ))
        );
        assert!(uploads.rates_for_test().aggregate_bps > 0);

        observer.on_blob_upload_progress(&blob, 1016, 1016).await;
        observer.on_blob_uploaded(&blob).await;
        assert_eq!(uploads.transient_state_for_test(&blob), None);
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);
    }

    /// An attempt whose drain was dropped mid-transfer reports its end, and
    /// the outbox stops showing it in flight; a later drain starts it afresh.
    #[tokio::test]
    async fn an_abandoned_attempt_leaves_nothing_in_flight() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 600, 1016).await;
        observer.on_blob_upload_abandoned(&blob);

        assert_eq!(uploads.transient_state_for_test(&blob), None);
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);
        observer.on_blob_upload_started(&blob).await;
    }

    #[tokio::test]
    async fn completed_preparation_can_enter_provider_upload() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_preparation_started(&blob).await;
        observer
            .on_blob_preparation_progress(&blob, 1000, 1000)
            .await;
        let rates = uploads.rates_for_test();
        assert!(rates.aggregate_bps > 0);
        assert_eq!(rates.provider_bps, 0);
        observer.on_blob_upload_started(&blob).await;

        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(TransientUploadState::UploadStarted)
        );
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);
    }

    #[tokio::test]
    async fn failed_preparation_ends_its_throughput_measurement() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_preparation_started(&blob).await;
        observer
            .on_blob_preparation_progress(&blob, 500, 1000)
            .await;
        assert!(uploads.rates_for_test().aggregate_bps > 0);

        observer
            .on_blob_upload_failed(&blob, "preparation failed")
            .await;
        assert_eq!(uploads.transient_state_for_test(&blob), None);
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);
    }

    /// Callbacks that contradict themselves leave the attempt untracked: its
    /// row renders coven's durable phase, and later reports of that attempt
    /// change nothing until the next start.
    #[tokio::test]
    async fn contradictory_provider_callbacks_leave_the_attempt_untracked() {
        for (name, script) in [
            ("progress without a start", vec![(600, 1016)]),
            ("progress beyond its total", vec![(1_017, 1_016)]),
            ("progress going backwards", vec![(600, 1016), (500, 1016)]),
            (
                "progress changing its total",
                vec![(600, 1016), (700, 1032)],
            ),
        ] {
            let (observer, uploads) = observer();
            let blob = test_blob();
            if name != "progress without a start" {
                observer.on_blob_upload_started(&blob).await;
            }
            for (done, total) in script {
                observer.on_blob_upload_progress(&blob, done, total).await;
            }

            assert_eq!(
                uploads.transient_state_for_test(&blob),
                Some(TransientUploadState::Untracked),
                "{name}"
            );
            assert_eq!(uploads.rates_for_test().aggregate_bps, 0, "{name}");
            observer.on_blob_upload_progress(&blob, 900, 1016).await;
            assert_eq!(
                uploads.transient_state_for_test(&blob),
                Some(TransientUploadState::Untracked),
                "{name}"
            );
            observer.on_blob_upload_abandoned(&blob);
            assert_eq!(uploads.transient_state_for_test(&blob), None, "{name}");
        }
    }

    #[tokio::test]
    async fn contradictory_preparation_callbacks_leave_the_attempt_untracked() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_preparation_started(&blob).await;
        observer.on_blob_preparation_progress(&blob, 500, 999).await;

        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(TransientUploadState::Untracked)
        );
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);
        observer.on_blob_upload_started(&blob).await;
        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(TransientUploadState::UploadStarted)
        );
    }

    /// Completion without the exact final provider report still clears the
    /// attempt: coven's durable Created row says Uploaded.
    #[tokio::test]
    async fn completion_without_the_final_progress_clears_the_attempt() {
        let (observer, uploads) = observer();
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_uploaded(&blob).await;

        assert_eq!(uploads.transient_state_for_test(&blob), None);
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);
    }
}
