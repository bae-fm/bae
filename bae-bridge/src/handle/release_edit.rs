use super::*;

/// One open release editor's session on a library release: the form with
/// each change the person made in it, what a catalog applied in it read and
/// the album picked in it, held in core's tables until its save. Every
/// editor opens its own, so two editors on one release never share an edit,
/// an apply or a pick; releasing the session deletes it, writing nothing to
/// the release. The editor reads the session's draft through
/// [`Self::draft_subscription`], and each command writes the session and
/// returns once its write is done.
#[derive(uniffi::Object)]
pub struct ReleaseEditSession {
    inner: bae_library::ReleaseEdit,
    services: AppServices,
    runtime: crate::operation_runtime::OperationRuntime,
}

forward! {
    #[cfg(desktop)]
    async this => {
        /// Open an edit session of its own on a library release: its form,
        /// seeded from the release's values as the library holds them, with
        /// nothing edited, applied or picked.
        fn open_release_edit(release_id: String) -> std::sync::Arc<ReleaseEditSession> {
            Ok(std::sync::Arc::new(ReleaseEditSession {
                inner: this.services.open_release_edit(&release_id).await?,
                services: this.services.clone(),
                runtime: this.runtime.clone(),
            }))
        }
    }
}

#[uniffi::export]
impl ReleaseEditSession {
    /// The session's draft as the editor shows it, now and after each write
    /// that changes it: each command's, an artist merged in the library, the
    /// album renamed, a cover picked. The subscription sends `None` once
    /// someone deletes the session's release.
    pub fn draft_subscription(&self) -> std::sync::Arc<crate::ReleaseEditDraftSubscription> {
        crate::screens::release_edit_draft(
            self.services.subscribe_release_edit_draft(&self.inner),
            self.runtime.clone(),
        )
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ReleaseEditSession {
    /// Write one album-level text field of the form as the person left it.
    pub async fn set_field(
        self: std::sync::Arc<Self>,
        field: crate::types::BridgeCandidateEditField,
        value: String,
    ) -> Result<(), BridgeError> {
        let edit = bae_import::DraftFieldEdit::Text { field: field.into_core(), value };
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_field", move || async move {
                Ok(self.services.edit_release_edit_field(&self.inner, edit).await?)
            })
            .await
    }

    /// Write the form's label rows as the person left them.
    pub async fn set_labels(
        self: std::sync::Arc<Self>,
        labels: Vec<crate::types::BridgeRawLabelEdit>,
    ) -> Result<(), BridgeError> {
        let edit = bae_import::DraftFieldEdit::Labels(
            labels.into_iter().map(crate::types::BridgeRawLabelEdit::into_core).collect(),
        );
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_labels", move || async move {
                Ok(self.services.edit_release_edit_field(&self.inner, edit).await?)
            })
            .await
    }

    /// Write the form's barcode rows as the person left them.
    pub async fn set_barcodes(
        self: std::sync::Arc<Self>,
        barcodes: Vec<String>,
    ) -> Result<(), BridgeError> {
        let edit = bae_import::DraftFieldEdit::Barcodes(barcodes);
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_barcodes", move || async move {
                Ok(self.services.edit_release_edit_field(&self.inner, edit).await?)
            })
            .await
    }

    /// Write one choice of what the pressing is.
    pub async fn set_pressing_fact(
        self: std::sync::Arc<Self>,
        fact: crate::types::BridgePressingFactEdit,
    ) -> Result<(), BridgeError> {
        let edit = bae_import::DraftFieldEdit::PressingFact(fact.into_core());
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_pressing_fact", move || async move {
                Ok(self.services.edit_release_edit_field(&self.inner, edit).await?)
            })
            .await
    }

    /// Replace the form's ordered album credits.
    pub async fn set_album_artists(
        self: std::sync::Arc<Self>,
        assignments: Vec<crate::types::BridgeCredited>,
    ) -> Result<(), BridgeError> {
        let assignments: Vec<_> =
            assignments.into_iter().map(crate::types::BridgeCredited::into_core).collect();
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_album_artists", move || async move {
                Ok(self.services.set_release_edit_album_artists(&self.inner, assignments).await?)
            })
            .await
    }

    /// Write one track row's title and artists as the person left them, by
    /// the row's id.
    pub async fn set_track(
        self: std::sync::Arc<Self>,
        track: crate::types::BridgeRawTrackEdit,
    ) -> Result<(), BridgeError> {
        let track = track.into_core();
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_track", move || async move {
                Ok(self.services.set_release_edit_track(&self.inner, track).await?)
            })
            .await
    }

    /// Apply the catalog release `link` picks to this session: every value it
    /// gives, held until the session saves, replaces the form. When the app
    /// shows the apply's step, the session writes each network read it
    /// waits on to `apply`'s row while the read runs.
    pub async fn apply_catalog(
        self: std::sync::Arc<Self>,
        link: crate::types::BridgePressingLink,
        apply: Option<std::sync::Arc<crate::BridgeCatalogApply>>,
    ) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.apply_catalog", move || async move {
                let steps = crate::screens::catalog_apply_steps(apply.as_deref());
                Ok(self
                    .services
                    .apply_release_edit_catalog(&self.inner, &link.into_core(), &steps)
                    .await?)
            })
            .await
    }

    /// Record what the person picked for the album this session files the
    /// release under: a library album, a new one, or the album its records
    /// name.
    pub async fn set_album(
        self: std::sync::Arc<Self>,
        pick: crate::types::BridgeAlbumPick,
    ) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_album", move || async move {
                Ok(self.services.set_release_edit_album(&self.inner, pick.into_core()).await?)
            })
            .await
    }

    /// Discard everything the session holds: the session opens again on the
    /// release as the library holds it now.
    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.cancel", move || async move {
                Ok(self.services.reopen_release_edit(&self.inner).await?)
            })
            .await
    }

    /// Save this session into the album it files the release under: every
    /// value, in one write, which opens the session again on what it saved.
    /// Returns the album the release is on after the save. Refused, writing
    /// nothing, when the form can't be saved, or when the release changed
    /// after the session opened on it
    /// (`BridgeErrorCategory::ReleaseChangedSinceOpened`).
    pub async fn save(self: std::sync::Arc<Self>) -> Result<String, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.save", move || async move {
                self.services.save_release_edit(&self.inner).await?;
                self.services
                    .get_album_id_for_release(self.inner.release_id())
                    .await
                    .map_err(BridgeError::from)
            })
            .await
    }
}
