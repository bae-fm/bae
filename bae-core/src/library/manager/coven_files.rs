//! Coven file surface for [`LibraryManager`]: file reference construction, byte reads
//! for release files and library images, library-image stores, cover-ref
//! lookups, pin state, and the per-namespace cache budgets. Moves bytes /
//! file references / pin state through the [`CovenHandle`] and [`Database`], distinct
//! from the sync-provider control in `sync.rs`.

use super::release::cover_ref_for;
use super::*;

impl LibraryManager {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn release_file_ref_for_test(
        &self,
        file_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        self.release_file_ref(file_id).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn materialize_release_file_for_test(
        &self,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        Ok(self.database.read_release_file(file_id).await?)
    }

    /// A library image's file as this device sees it, for tests that check
    /// what it holds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn library_image_file_for_test(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, LibraryError> {
        self.library_image_file(id, image_type).await
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn evict_file_for_test(&self, file: &coven::FileRef) -> Result<(), LibraryError> {
        self.database
            .evict_file(file)
            .await
            .map_err(|error| LibraryError::file_read("evict", error))
    }

    /// Apply [`crate::sync::CACHE_BUDGETS`] to coven for this device.
    /// Device-local; set once at startup.
    pub(crate) async fn configure_cache_budgets(&self) -> Result<(), LibraryError> {
        for (namespace, budget) in crate::sync::CACHE_BUDGETS {
            self.database.set_cache_budget(namespace, budget).await?;
        }
        Ok(())
    }

    /// The exact-row file reference coven binds for a release file's audio file,
    /// captured from the live `release_files` row. coven's cache ops (read, pin,
    /// stream) key on this so a later row replacement cannot redirect a read; a
    /// missing row surfaces as an error.
    pub(crate) async fn release_file_ref(
        &self,
        file_id: &str,
    ) -> Result<coven::FileRef, LibraryError> {
        self.database
            .file_ref(crate::sync::RELEASE_FILES_NAMESPACE, file_id)
            .await
            .map_err(|e| {
                LibraryError::Storage(format!("file reference for release file {file_id}: {e}"))
            })
    }

    /// How many of these release files carry a file on this device in coven:
    /// this device's pending attachment, or the stored file the row names.
    pub(crate) async fn attached_release_file_count(
        &self,
        files: &[DbFile],
    ) -> Result<usize, LibraryError> {
        let mut attached = 0;
        for file in files {
            if self
                .database
                .current_file(crate::sync::RELEASE_FILES_NAMESPACE, &file.id)
                .await?
                .is_some()
            {
                attached += 1;
            }
        }
        Ok(attached)
    }

    /// Read through the same local-or-attached source used for playback.
    pub(crate) async fn read_release_file(&self, file: &DbFile) -> Result<Vec<u8>, LibraryError> {
        Ok(self.database.read_release_file(&file.id).await?)
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// cover's version — or `None` when the release has no cover row.
    pub async fn cover_ref(&self, release_id: &str) -> Result<Option<ImageRef>, LibraryError> {
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
    pub async fn read_image_bytes(
        &self,
        image: &ImageRef,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_bytes(&image.id, &image.image_type)
            .await
    }

    pub async fn read_cover_image_bytes(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        self.read_library_image_bytes(release_id, &LibraryImageType::Cover)
            .await
    }

    /// Read a library image's whole bytes by its subject id and kind; `None`
    /// when this device has no file for it.
    pub async fn read_library_image_bytes(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        let Some(file) = self.library_image_file(id, image_type).await? else {
            return Ok(None);
        };
        let bytes = self
            .database
            .read_file(&file)
            .await
            .map_err(|e| LibraryError::file_read(format!("read image {id}"), e))?;
        Ok(Some(bytes))
    }

    /// A library image's file as this device sees it: its own pending
    /// attachment, or the stored file the row names. `None` when there is no
    /// such row, or another device hasn't finished uploading its file.
    async fn library_image_file(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<coven::FileRef>, LibraryError> {
        Ok(self
            .database
            .current_file(image_type.namespace(), id)
            .await?)
    }

    /// A library image to show: its bytes. `None` when this device has no
    /// file for it.
    pub async fn read_image(&self, image: &ImageRef) -> Result<Option<ImageRead>, LibraryError> {
        let Some(file) = self
            .library_image_file(&image.id, &image.image_type)
            .await?
        else {
            return Ok(None);
        };
        let bytes =
            self.database.read_file(&file).await.map_err(|error| {
                LibraryError::file_read(format!("read image {}", image.id), error)
            })?;
        Ok(Some(ImageRead::Bytes(bytes)))
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
                        kind: crate::diagnostics::AnomalyKind::FileIdInvalid,
                    });
                    false
                }
            })
            .collect()
    }

    /// Audio and companion files stay pinned together. Covers use their own
    /// cache budget and are never pinned.
    pub(super) async fn release_pinnable_files(
        &self,
        release_id: &str,
    ) -> Result<Vec<PinnableFile>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let mut pinnable = Vec::with_capacity(files.len());
        for file in &files {
            pinnable.push(PinnableFile {
                file: self.release_file_ref(&file.id).await?,
                bytes: pinnable_bytes(release_id, &file.id, file.file_size)?,
            });
        }
        Ok(pinnable)
    }

    /// The Downloads pane's opening progress for a queued pin: zero of the same
    /// total [`Self::pin_release_files_with_progress`] counts down, so the bar
    /// never jumps when the pin starts reporting.
    pub(crate) async fn initial_download_progress(
        &self,
        release_id: &str,
    ) -> Result<crate::library::DownloadTransferProgress, LibraryError> {
        let pinnable = self.release_pinnable_files(release_id).await?;
        crate::library::DownloadTransferProgress::new(
            release_id,
            0,
            total_bytes(release_id, &pinnable)?,
        )
    }

    /// Unpin a remote release's files: coven moves every file from
    /// the pinned cache to the evictable cache (still readable, now
    /// droppable). No cloud read, no bae flag. The low-level cache op behind the
    /// "Unpin" transition.
    pub(crate) async fn unpin_release_files(&self, release_id: &str) -> Result<(), LibraryError> {
        let files: Vec<coven::FileRef> = self
            .release_pinnable_files(release_id)
            .await?
            .into_iter()
            .map(|entry| entry.file)
            .collect();
        self.database
            .unpin(&files)
            .await
            .map_err(|e| LibraryError::file_read(format!("unpin release {release_id}"), e))
    }

    /// Store a bae-produced host-provided image and its row in one coven batch.
    pub async fn store_library_image(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.database.write_library_image(image, bytes).await?;
        Ok(())
    }
}

/// An image to show, read now or still on its way from another device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageRead {
    Bytes(Vec<u8>),
    /// Another device hasn't uploaded the image's file yet. No read returns
    /// this: a row names its file only once storage holds it.
    Arriving(super::ArrivingFile),
}

/// One file of a release's pinned set, with the plaintext byte size its row
/// records — what a pin fetches, and what the Downloads pane counts.
pub(super) struct PinnableFile {
    pub(super) file: coven::FileRef,
    bytes: u64,
}

fn pinnable_bytes(release_id: &str, row_id: &str, file_size: i64) -> Result<u64, LibraryError> {
    u64::try_from(file_size).map_err(|error| {
        LibraryError::Storage(format!(
            "pin release {release_id}: row {row_id} has negative size: {error}"
        ))
    })
}

fn total_bytes(release_id: &str, pinnable: &[PinnableFile]) -> Result<u64, LibraryError> {
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
