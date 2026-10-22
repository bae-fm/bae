use super::*;

/// One open release editor's session on a library release: what a catalog
/// applied in it read and the album picked in it, held until its save. Every
/// editor opens its own, so two editors on one release never share an apply
/// or a pick; releasing the session discards it, writing nothing.
#[derive(uniffi::Object)]
pub struct ReleaseEditSession {
    inner: bae_core::library::ReleaseEditSession,
    services: AppServices,
    runtime: tokio::runtime::Handle,
}

forward! {
    #[cfg(feature = "desktop")]
    async this => {
        /// Open an edit session of its own on a library release: its form,
        /// seeded from the release's values as the library holds them, with
        /// nothing applied or picked.
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
    /// The release as the library held it when this session opened: the
    /// form, cover, tracks and album the editor first shows.
    pub fn opened(&self) -> crate::types::BridgeReleaseEditSeed {
        crate::types::BridgeReleaseEditSeed::from_core(self.inner.opened().clone())
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ReleaseEditSession {
    /// Apply the catalog release `link` picks to this session: every value it
    /// gives, held until the session saves. Returns the editor's form filled
    /// with them.
    pub async fn apply_catalog(
        self: std::sync::Arc<Self>,
        link: crate::types::BridgePressingLink,
    ) -> Result<crate::types::BridgeReleaseEditSeed, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            let seed = self
                .services
                .apply_release_edit_catalog(&self.inner, &link.into_core())
                .await?;
            Ok(crate::types::BridgeReleaseEditSeed::from_core(seed))
        })
        .await
    }

    /// Pick the library album this session files the release under, over the
    /// one its records name, or (`None`) leave it to its records. Returns the
    /// album the session now files it under.
    pub async fn set_album(
        self: std::sync::Arc<Self>,
        album_id: Option<String>,
    ) -> Result<crate::types::BridgeDraftAlbum, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            let album = self
                .services
                .set_release_edit_album(&self.inner, album_id)
                .await?;
            Ok(crate::types::BridgeDraftAlbum::from_core(album))
        })
        .await
    }

    /// Save this session with the form the person left, into the album the
    /// session files it under: every value, in one write. Returns the album
    /// the release is on after the save. Refused, writing nothing, when the
    /// release changed after the session opened on it
    /// (`BridgeErrorCategory::ReleaseChangedSinceOpened`).
    pub async fn save(
        self: std::sync::Arc<Self>,
        edit: crate::types::BridgeRawReleaseEdit,
    ) -> Result<String, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.services
                .save_release_edit(&self.inner, &edit.into_core())
                .await?;
            self.services
                .get_album_id_for_release(self.inner.release_id())
                .await
                .map_err(BridgeError::database)
        })
        .await
    }
}
