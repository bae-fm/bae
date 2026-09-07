use super::*;

#[uniffi::export]
impl AppHandle {
    /// A fresh token, generated rather than read: nothing about the handle
    /// takes part.
    pub fn generate_mcp_token(&self) -> String {
        bae_core::library::generate_mcp_token()
    }
}

forward! { sync this => {
    fn get_mcp_token() -> Result<String, BridgeError> {
        Ok(this.services.ensure_mcp_token()?)
    }

    fn set_mcp_token(token: String) -> Result<(), BridgeError> {
        this.services.set_mcp_token(token)?;
        Ok(())
    }

    fn remove_discogs_token() -> Result<(), BridgeError> {
        this.services
            .import_remove_discogs_token()
            .map_err(BridgeError::config)
    }
} }

forward! { async this => {
    fn set_mcp_server_config(enabled: bool, port: u16) -> () {
        this.desktop
            .set_mcp_config(bae_core::config::McpConfig { enabled, port })
            .await
            .map_err(BridgeError::config)
    }

    fn get_mcp_server_status() -> BridgeMcpServerStatus {
        Ok(BridgeMcpServerStatus::from_core(
            this.desktop.mcp_server_status().await,
        ))
    }

    fn set_subsonic_server_config(
        enabled: bool,
        port: u16,
        username: String,
        bind_address: String,
    ) -> () {
        this.desktop
            .set_subsonic_config(bae_core::config::SubsonicConfig {
                enabled,
                port,
                username,
                bind_address,
            })
            .await
            .map_err(BridgeError::config)
    }

    fn get_subsonic_server_status() -> BridgeSubsonicServerStatus {
        Ok(BridgeSubsonicServerStatus::from_core(
            this.desktop.subsonic_server_status().await,
        ))
    }

    fn set_subsonic_password(password: String) -> () {
        this.desktop
            .set_subsonic_password(&password)
            .await
            .map_err(BridgeError::config)
    }

    /// Validate then persist a Discogs API token, returning what happened so the
    /// UI can react (keep the draft on `Rejected`, show the optimistic-save note
    /// on `Unvalidated`). Lives on the import service, which only runs on desktop
    /// (identification). Mobile reads token status via `get_config` but never
    /// writes.
    fn save_discogs_token(token: String) -> BridgeDiscogsSaveOutcome {
        this.services
            .import_save_discogs_token(&token)
            .await
            .map(BridgeDiscogsSaveOutcome::from_core)
            .map_err(BridgeError::config)
    }

    /// Re-check a stored `Unvalidated` key against Discogs. No-op when no key is
    /// stored or it's already settled. Called at app launch and settings-tab
    /// open for the offline-saved case.
    fn revalidate_discogs_token() -> () {
        this.services
            .import_revalidate_discogs_token()
            .await
            .map_err(BridgeError::config)
    }
} }
