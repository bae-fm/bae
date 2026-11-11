use super::*;

#[uniffi::export]
impl AppHandle {
    /// A fresh token, generated rather than read: nothing about the handle
    /// takes part.
    pub fn generate_mcp_token(&self) -> String {
        bae_library::generate_mcp_token()
    }
}

forward! { async this => {
    /// The MCP bearer token, created the first time it is asked for. A
    /// keychain call, so it runs off the caller's thread.
    fn get_mcp_token() -> String {
        Ok(this.services.ensure_mcp_token().await?)
    }

    fn set_mcp_token(token: String) -> () {
        this.services.set_mcp_token(token).await?;
        Ok(())
    }
} }

#[uniffi::export]
impl AppHandle {
    /// The Subsonic server besides the settings: what it is doing, the
    /// networks to choose from and whether a password is saved. The first
    /// value at once, then one each time any of it changes.
    pub fn subscribe_subsonic(&self) -> std::sync::Arc<crate::SubsonicSubscription> {
        crate::screens::subsonic(self.desktop.subscribe_subsonic(), self.runtime.clone())
    }
}

forward! { async this => {
    fn set_mcp_server_config(enabled: bool, port: u16) -> () {
        this.desktop
            .set_mcp_config(bae_preferences::McpConfig { enabled, port })
            .await
            .map_err(BridgeError::config)
    }

    fn get_mcp_server_status() -> BridgeServerStatus {
        Ok(BridgeServerStatus::from_core(
            this.desktop.mcp_server_status().await,
        ))
    }

    fn set_subsonic_enabled(enabled: bool) -> () {
        Ok(this.desktop.set_subsonic_enabled(enabled).await?)
    }

    fn set_subsonic_port(port: u32) -> () {
        Ok(this.desktop.set_subsonic_port(port).await?)
    }

    fn set_subsonic_username(username: String) -> () {
        Ok(this.desktop.set_subsonic_username(username).await?)
    }

    fn set_subsonic_network(network: crate::types::BridgeSubsonicNetwork) -> () {
        Ok(this.desktop.set_subsonic_network(network.into_core()).await?)
    }

    fn set_subsonic_password(password: String) -> () {
        Ok(this.desktop.set_subsonic_password(&password).await?)
    }

    /// Validate then persist a Discogs API token, returning what happened so the
    /// UI can react (keep the draft on `Rejected`, show the optimistic-save note
    /// on `Unvalidated`). Lives on the import service, which only runs on desktop
    /// (identification). Mobile reads token status from `subscribe_config`
    /// but never writes.
    fn save_discogs_token(token: String) -> BridgeDiscogsSaveOutcome {
        this.services
            .import_save_discogs_token(&token)
            .await
            .map(BridgeDiscogsSaveOutcome::from_core)
            .map_err(BridgeError::from)
    }

    /// Re-check a stored `Unvalidated` key against Discogs. No-op when no key is
    /// stored or it's already settled. Called at app launch and settings-tab
    /// open for the offline-saved case.
    fn revalidate_discogs_token() -> () {
        this.services
            .import_revalidate_discogs_token()
            .await
            .map_err(BridgeError::from)
    }
} }
