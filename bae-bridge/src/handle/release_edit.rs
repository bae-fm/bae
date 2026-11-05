use super::*;

/// One open release editor's session on a library release: the form with
/// each change the person made in it, what a catalog applied in it read and
/// the album picked in it, held until its save. Every editor opens its own,
/// so two editors on one release never share an edit, an apply or a pick;
/// releasing the session discards it, writing nothing. Each change returns
/// the session's draft, what the editor shows.
#[derive(uniffi::Object)]
pub struct ReleaseEditSession {
    inner: bae_library::ReleaseEdit,
    services: AppServices,
    runtime: crate::operation_runtime::OperationRuntime,
}

forward! {
    #[cfg(feature = "desktop")]
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

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ReleaseEditSession {
    /// The session's draft as the editor shows it now, with the release's
    /// cover as it stands, which the cover picker writes outside the session.
    pub async fn draft(
        self: std::sync::Arc<Self>,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.draft", move || async move {
                let draft = self.services.release_edit_draft(&self.inner).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Write one album-level text field of the form as the person left it.
    pub async fn set_field(
        self: std::sync::Arc<Self>,
        field: crate::types::BridgeCandidateEditField,
        value: String,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let edit = bae_import::DraftFieldEdit::Text { field: field.into_core(), value };
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_field", move || async move {
                let draft = self.services.edit_release_edit_field(&self.inner, edit).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Write the form's label rows as the person left them.
    pub async fn set_labels(
        self: std::sync::Arc<Self>,
        labels: Vec<crate::types::BridgeRawLabelEdit>,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let edit = bae_import::DraftFieldEdit::Labels(
            labels.into_iter().map(crate::types::BridgeRawLabelEdit::into_core).collect(),
        );
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_labels", move || async move {
                let draft = self.services.edit_release_edit_field(&self.inner, edit).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Write the form's barcode rows as the person left them.
    pub async fn set_barcodes(
        self: std::sync::Arc<Self>,
        barcodes: Vec<String>,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let edit = bae_import::DraftFieldEdit::Barcodes(barcodes);
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_barcodes", move || async move {
                let draft = self.services.edit_release_edit_field(&self.inner, edit).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Write one choice of what the pressing is.
    pub async fn set_pressing_fact(
        self: std::sync::Arc<Self>,
        fact: crate::types::BridgePressingFactEdit,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let edit = bae_import::DraftFieldEdit::PressingFact(fact.into_core());
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_pressing_fact", move || async move {
                let draft = self.services.edit_release_edit_field(&self.inner, edit).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Replace the form's ordered album credits.
    pub async fn set_album_artists(
        self: std::sync::Arc<Self>,
        assignments: Vec<crate::types::BridgeCredited>,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let assignments: Vec<_> =
            assignments.into_iter().map(crate::types::BridgeCredited::into_core).collect();
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_album_artists", move || async move {
                let draft =
                    self.services.set_release_edit_album_artists(&self.inner, assignments).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Write one track row's title and artists as the person left them, by
    /// the row's id.
    pub async fn set_track(
        self: std::sync::Arc<Self>,
        track: crate::types::BridgeRawTrackEdit,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let track = track.into_core();
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_track", move || async move {
                let draft = self.services.set_release_edit_track(&self.inner, track).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Apply the catalog release `link` picks to this session: every value it
    /// gives, held until the session saves, replaces the form. Each network
    /// read it waits on is written to `apply`'s stream, when the app shows
    /// the apply.
    pub async fn apply_catalog(
        self: std::sync::Arc<Self>,
        link: crate::types::BridgePressingLink,
        apply: Option<std::sync::Arc<crate::BridgeCatalogApply>>,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.apply_catalog", move || async move {
                let steps = crate::screens::catalog_apply_steps(apply.as_deref());
                let draft = self
                    .services
                    .apply_release_edit_catalog(&self.inner, &link.into_core(), &steps)
                    .await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Pick the library album this session files the release under, over the
    /// one its records name, or (`None`) leave it to its records. The draft
    /// names the album the session now files it under.
    pub async fn set_album(
        self: std::sync::Arc<Self>,
        album_id: Option<String>,
    ) -> Result<crate::types::BridgeReleaseEditDraft, BridgeError> {
        let runtime = self.runtime.clone();
        runtime
            .run("ReleaseEditSession.set_album", move || async move {
                let draft = self.services.set_release_edit_album(&self.inner, album_id).await?;
                Ok(crate::types::BridgeReleaseEditDraft::from_core(draft))
            })
            .await
    }

    /// Save this session into the album it files the release under: every
    /// value, in one write. Returns the album the release is on after the
    /// save. Refused, writing nothing, when the form can't be saved, or when
    /// the release changed after the session opened on it
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
