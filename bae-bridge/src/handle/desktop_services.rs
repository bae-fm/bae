use super::*;

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub async fn set_mcp_server_config(
        self: std::sync::Arc<Self>,
        enabled: bool,
        port: u16,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.desktop
                .set_mcp_config(bae_core::config::McpConfig { enabled, port })
                .await
                .map_err(BridgeError::config)
        })
        .await
    }

    pub async fn get_mcp_server_status(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeMcpServerStatus, BridgeError> {
        self.run_exported(move |this| async move {
            Ok(BridgeMcpServerStatus::from_core(
                this.desktop.mcp_server_status().await,
            ))
        })
        .await
    }

    pub fn get_mcp_token(&self) -> Result<String, BridgeError> {
        Ok(self.services.ensure_mcp_token()?)
    }

    pub fn generate_mcp_token(&self) -> String {
        bae_core::library::generate_mcp_token()
    }

    pub fn set_mcp_token(&self, token: String) -> Result<(), BridgeError> {
        self.services.set_mcp_token(token)?;
        Ok(())
    }

    pub async fn set_subsonic_server_config(
        self: std::sync::Arc<Self>,
        enabled: bool,
        port: u16,
        username: String,
        bind_address: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.desktop
                .set_subsonic_config(bae_core::config::SubsonicConfig {
                    enabled,
                    port,
                    username,
                    bind_address,
                })
                .await
                .map_err(BridgeError::config)
        })
        .await
    }

    pub async fn get_subsonic_server_status(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeSubsonicServerStatus, BridgeError> {
        self.run_exported(move |this| async move {
            Ok(BridgeSubsonicServerStatus::from_core(
                this.desktop.subsonic_server_status().await,
            ))
        })
        .await
    }

    pub async fn set_subsonic_password(
        self: std::sync::Arc<Self>,
        password: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.desktop
                .set_subsonic_password(&password)
                .await
                .map_err(BridgeError::config)
        })
        .await
    }

    /// Validate then persist a Discogs API token, returning what happened so the
    /// UI can react (keep the draft on `Rejected`, show the optimistic-save note
    /// on `Unvalidated`). Lives on the import service, which only runs on desktop
    /// (identification). Mobile reads token status via `get_config` but never
    /// writes.
    pub async fn save_discogs_token(
        self: std::sync::Arc<Self>,
        token: String,
    ) -> Result<BridgeDiscogsSaveOutcome, BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_save_discogs_token(&token)
                .await
                .map(BridgeDiscogsSaveOutcome::from_core)
                .map_err(BridgeError::config)
        })
        .await
    }

    /// Re-check a stored `Unvalidated` key against Discogs. No-op when no key is
    /// stored or it's already settled. Called at app launch and settings-tab
    /// open for the offline-saved case.
    pub async fn revalidate_discogs_token(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .import_revalidate_discogs_token()
                .await
                .map_err(BridgeError::config)
        })
        .await
    }

    pub fn remove_discogs_token(&self) -> Result<(), BridgeError> {
        self.services
            .import_remove_discogs_token()
            .map_err(BridgeError::config)
    }
}
