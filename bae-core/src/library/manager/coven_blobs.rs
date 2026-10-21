//! Coven blob surface for [`LibraryManager`]: blob-ref construction, byte reads
//! for release files and library images, library-image stores, cover-ref
//! lookups, pin state, and the per-namespace cache budgets. Moves bytes /
//! blob-refs / pin state through the [`CovenHandle`] and [`Database`], distinct
//! from the sync-provider control in `sync.rs`.

use super::release::cover_ref_for;
use super::*;

impl LibraryManager {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn release_blob_ref_for_test(
        &self,
        file_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        self.release_file_row_blob_ref(file_id).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn materialize_release_blob_for_test(
        &self,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        Ok(self.database.read_release_file(file_id).await?)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn evict_blob_for_test(&self, blob: &coven::FileRef) -> Result<(), LibraryError> {
        self.database
            .evict_file(blob)
            .await
            .map_err(|error| LibraryError::blob("evict", error))
    }

    /// Apply [`crate::sync::CACHE_BUDGETS`] to coven for this device.
    /// Device-local; set once at startup.
    pub(crate) async fn configure_cache_budgets(&self) -> Result<(), LibraryError> {
        for (namespace, budget) in crate::sync::CACHE_BUDGETS {
            self.database.set_cache_budget(namespace, budget).await?;
        }
        Ok(())
    }

    /// The exact-row blob reference coven binds for a release file's audio blob,
    /// captured from the live `release_files` row. coven's cache ops (read, pin,
    /// stream) key on this so a later row replacement cannot redirect a read; a
    /// missing row surfaces as an error.
    pub(crate) async fn release_file_row_blob_ref(
        &self,
        file_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        self.database
            .file_ref(crate::sync::RELEASE_FILES_NAMESPACE, file_id)
            .await
            .map_err(|e| LibraryError::Storage(format!("blob ref for release file {file_id}: {e}")))
    }

    /// The exact-row blob reference for a host-provided image, keyed by its table
    /// (`covers` / `artist_images`) and its subject row id (the release / artist).
    async fn image_row_blob_ref(
        &self,
        table: &str,
        row_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        self.database
            .file_ref(table, row_id)
            .await
            .map_err(|e| LibraryError::Storage(format!("blob ref for {table} {row_id}: {e}")))
    }

    /// Read through the same local-or-attached source used for playback.
    pub(crate) async fn read_release_blob(&self, file: &DbFile) -> Result<Vec<u8>, LibraryError> {
        Ok(self.database.read_release_file(&file.id).await?)
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// `covers` row's `blob_id` — or `None` when the release has no cover row.
    pub(crate) async fn cover_ref(
        &self,
        release_id: &str,
    ) -> Result<Option<ImageRef>, LibraryError> {
        cover_ref_for(&self.database, release_id).await
    }

    /// The cover [`ImageRef`] for each of `release_ids` that has a `covers` row,
    /// in one query. The batch source for the list/grid resolvers, which build a
    /// `Fn(&str) -> Option<ImageRef>` over the returned map.
    pub(super) async fn cover_refs(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(image_refs(
            self.database.cover_versions(release_ids).await?,
            LibraryImageType::Cover,
        ))
    }

    pub(super) async fn artist_image_refs(
        &self,
        artist_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(image_refs(
            self.database.artist_image_versions(artist_ids).await?,
            LibraryImageType::Artist,
        ))
    }

    /// Read a host-provided library image's whole bytes through coven's
    /// locality-aware read. The [`ImageRef`] carries the image kind, so the read
    /// goes directly to the table/namespace that produced the ref.
    pub async fn read_image_blob(&self, image: &ImageRef) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_blob(&image.id, &image.image_type)
            .await
    }

    pub async fn read_cover_image_blob(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_blob(release_id, &LibraryImageType::Cover)
            .await
    }

    async fn read_library_image_blob(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        let Some(blob) = self.library_image_file(id, image_type).await? else {
            return Ok(None);
        };
        let bytes = self
            .database
            .read_file(&blob)
            .await
            .map_err(|e| LibraryError::blob(format!("read image {id}"), e))?;
        Ok(Some(bytes))
    }

    /// A library image's file, or `None` when no image is stored: the image
    /// row must exist for coven to bind its row's file.
    async fn library_image_file(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, LibraryError> {
        if self
            .database
            .find_library_image(id, image_type)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        Ok(Some(
            self.image_row_blob_ref(image_type.namespace(), id).await?,
        ))
    }

    /// A library image to show: its bytes, or the file another device is
    /// still uploading, which arrives by itself (`wait_for_arrival`).
    /// `None` when no image is stored.
    pub async fn read_image(&self, image: &ImageRef) -> Result<Option<ImageRead>, LibraryError> {
        let Some(file) = self
            .library_image_file(&image.id, &image.image_type)
            .await?
        else {
            return Ok(None);
        };
        match self.database.read_file(&file).await {
            Ok(bytes) => Ok(Some(ImageRead::Bytes(bytes))),
            Err(coven::FileReadError::Uploading { .. }) => {
                Ok(Some(ImageRead::Arriving(arriving(&image.id, &file))))
            }
            Err(error) => Err(LibraryError::blob(
                format!("read image {}", image.id),
                error,
            )),
        }
    }

    /// A release is pinned only when every one of its files is pinned.
    pub(crate) async fn release_pinned(
        &self,
        any_file_id: Option<&str>,
    ) -> Result<bool, LibraryError> {
        Ok(self.releases_pinned(&[any_file_id]).await?[0])
    }

    /// The same answer for a list of releases, in the order given, in one read.
    /// Every list that shows a pin marker per row asks here once for the page
    /// rather than once per row: a page of releases costs one round trip to
    /// coven, not one per release.
    ///
    /// Each entry is a release's representative file id, or `None` for a release
    /// with no files.
    pub(crate) async fn releases_pinned(
        &self,
        any_file_ids: &[Option<&str>],
    ) -> Result<Vec<bool>, LibraryError> {
        Ok(self.pin_markers(release_file_pin_states(&self.database, any_file_ids).await?))
    }

    /// Watch whether each release of a view is pinned: a subscription whose
    /// rows are pointed at a view's releases with [`ReleasePinWatch::watch`] and
    /// that answers again whenever a pin, an unpin, an upload keeping its copy,
    /// or a release going Remote or Local changes the answer.
    pub(crate) fn watch_release_pins(&self) -> ReleasePinWatch {
        ReleasePinWatch {
            manager: self.clone(),
            query: None,
            file_query: None,
            watched: Vec::new(),
            groups: Vec::new(),
            answers: None,
        }
    }

    /// The pin marker each release shows. A file id that names no release file
    /// is counted as an anomaly and shows as not pinned.
    fn pin_markers(&self, states: Vec<ReleasePinState>) -> Vec<bool> {
        states
            .into_iter()
            .map(|state| match state {
                ReleasePinState::Pinned => true,
                ReleasePinState::NotPinned => false,
                ReleasePinState::RejectedBadId => {
                    self.diagnostics.event(TelemetryEvent::Anomaly {
                        kind: crate::diagnostics::AnomalyKind::BlobIdInvalid,
                    });
                    false
                }
            })
            .collect()
    }

    /// Audio and companion files stay pinned together. Covers use their own
    /// eager cache budget and are never pinned.
    pub(super) async fn release_pinnable_blobs(
        &self,
        release_id: &str,
    ) -> Result<Vec<PinnableBlob>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let mut pinnable = Vec::with_capacity(files.len());
        for file in &files {
            pinnable.push(PinnableBlob {
                blob: self.release_file_row_blob_ref(&file.id).await?,
                bytes: pinnable_bytes(release_id, &file.id, file.file_size)?,
            });
        }
        Ok(pinnable)
    }

    /// The Downloads pane's opening progress for a queued pin: zero of the same
    /// total [`Self::pin_release_blobs_with_progress`] counts down, so the bar
    /// never jumps when the pin starts reporting.
    pub(crate) async fn initial_download_progress(
        &self,
        release_id: &str,
    ) -> Result<crate::library::DownloadTransferProgress, LibraryError> {
        let pinnable = self.release_pinnable_blobs(release_id).await?;
        crate::library::DownloadTransferProgress::new(
            release_id,
            0,
            total_bytes(release_id, &pinnable)?,
        )
    }

    /// Unpin a remote release's blobs: coven moves every blob from
    /// the pinned cache to the evictable cache (still readable, now
    /// droppable). No cloud read, no bae flag. The low-level cache op behind the
    /// "Unpin" transition.
    pub(crate) async fn unpin_release_blobs(&self, release_id: &str) -> Result<(), LibraryError> {
        let blobs: Vec<coven::FileRef> = self
            .release_pinnable_blobs(release_id)
            .await?
            .into_iter()
            .map(|entry| entry.blob)
            .collect();
        self.database
            .unpin(&blobs)
            .await
            .map_err(|e| LibraryError::blob(format!("unpin release {release_id}"), e))
    }

    /// Store a bae-produced host-provided image and its row in one coven batch.
    pub async fn store_library_image_blob(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.database.write_library_image_blob(image, bytes).await?;
        Ok(())
    }
}

/// An image to show, read now or still on its way from another device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageRead {
    Bytes(Vec<u8>),
    /// Another device hasn't uploaded the image's file yet; it is no failure,
    /// and the image can be read once the file arrives.
    Arriving(super::ArrivingFile),
}

/// The file `image` names as arriving, noted once at debug: an image another
/// device is still uploading is the library arriving, not a failure.
pub(super) fn arriving(image: &str, file: &coven::FileRef) -> super::ArrivingFile {
    let arriving = super::ArrivingFile {
        uploader: file.uploader(),
        file: file.id(),
    };
    debug!(image, ?arriving, "image still arriving from its uploader");
    arriving
}

/// One blob of a release's pinned set, with the plaintext byte size its row
/// records — what a pin fetches, and what the Downloads pane counts.
pub(super) struct PinnableBlob {
    pub(super) blob: coven::FileRef,
    bytes: u64,
}

fn pinnable_bytes(release_id: &str, row_id: &str, file_size: i64) -> Result<u64, LibraryError> {
    u64::try_from(file_size).map_err(|error| {
        LibraryError::Storage(format!(
            "pin release {release_id}: row {row_id} has negative size: {error}"
        ))
    })
}

fn total_bytes(release_id: &str, pinnable: &[PinnableBlob]) -> Result<u64, LibraryError> {
    pinnable.iter().try_fold(0u64, |total, entry| {
        total.checked_add(entry.bytes).ok_or_else(|| {
            LibraryError::Storage(format!("pin release {release_id}: byte total overflow"))
        })
    })
}

type ReleaseFileGroups = Vec<Option<Vec<String>>>;

/// Follows both release file membership and coven's per-file pin answers.
pub(crate) struct ReleasePinWatch {
    manager: LibraryManager,
    query: Option<coven::LiveQuery<Vec<Option<bool>>>>,
    file_query: Option<coven::ReconfigurableLiveQuery<Vec<Option<String>>, ReleaseFileGroups>>,
    watched: Vec<Option<String>>,
    groups: ReleaseFileGroups,
    answers: Option<Vec<Option<bool>>>,
}

impl ReleasePinWatch {
    pub(crate) async fn watch(
        &mut self,
        files: Vec<Option<String>>,
    ) -> Result<Vec<bool>, LibraryError> {
        if self.file_query.is_none() || self.watched != files {
            self.watched = files.clone();
            self.file_query = Some(self.manager.database.subscribe_release_pin_files(files));
            self.query = None;
            self.answers = None;
        }
        if self.answers.is_some() {
            return Ok(self.markers());
        }
        self.changed().await
    }

    pub(crate) async fn changed(&mut self) -> Result<Vec<bool>, LibraryError> {
        if self.file_query.is_none() {
            return std::future::pending().await;
        }
        if self.query.is_none() {
            let event = self
                .file_query
                .as_mut()
                .expect("file query installed")
                .next()
                .await;
            self.install_groups(event.result?);
        }
        if self.answers.is_none() {
            return self.read_answers().await;
        }
        let file_query = self.file_query.as_mut().expect("file query installed");
        let query = self.query.as_mut().expect("pin query installed");
        tokio::select! {
            event = file_query.next() => {
                self.install_groups(event.result?);
                self.read_answers().await
            }
            answers = query.next() => {
                self.answers = Some(answers?);
                Ok(self.markers())
            }
        }
    }

    fn install_groups(&mut self, groups: ReleaseFileGroups) {
        self.query = Some(
            self.manager
                .database
                .subscribe_rows_pinned(crate::sync::RELEASE_FILES_NAMESPACE, pin_file_ids(&groups)),
        );
        self.groups = groups;
        self.answers = None;
    }

    async fn read_answers(&mut self) -> Result<Vec<bool>, LibraryError> {
        self.answers = Some(
            self.query
                .as_mut()
                .expect("pin query installed")
                .next()
                .await?,
        );
        Ok(self.markers())
    }

    fn markers(&self) -> Vec<bool> {
        self.manager.pin_markers(release_pin_states_from_answers(
            &self.groups,
            self.answers.as_ref().expect("pin answers read"),
        ))
    }
}
