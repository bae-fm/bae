#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDropboxAccess {
    AppFolder,
    FullDropbox,
}
mirror_enum! {
    BridgeDropboxAccess = coven::DropboxAccess,
    from_core: fn,
    variants: { AppFolder, FullDropbox },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOAuthAccess {
    Dropbox { access: BridgeDropboxAccess },
    OneDrive,
}
mirror_enum! {
    BridgeOAuthAccess = coven::OAuthAccess,
    from_core: pub(crate) fn,
    variants: { Dropbox(access: (BridgeDropboxAccess)), OneDrive },
}

/// Native sign-in returns the complete redirect, never provider tokens.
/// Cancelling the foreign future must dismiss the native sign-in sheet.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait OAuthPresenterCallback: Send + Sync {
    async fn present(
        &self,
        authorization_url: String,
    ) -> Result<String, BridgeOAuthPresentationError>;
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BridgeOAuthPresentationError {
    #[error("sign-in cancelled")]
    Cancelled,
    #[error("native sign-in failed: {detail}")]
    Failed { detail: String },
}

/// Redirects registered with the provider for the native app.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOAuthRedirects {
    pub dropbox: String,
    pub onedrive: String,
}

#[cfg(any(test, all(feature = "oauth-providers", not(desktop))))]
pub(crate) struct NativeOAuthPresenter {
    redirects: BridgeOAuthRedirects,
    callback: Box<dyn OAuthPresenterCallback>,
    /// Where a throw the callback doesn't declare is reported.
    diagnostics: bae_diagnostics::Diagnostics,
}

#[cfg(any(test, all(feature = "oauth-providers", not(desktop))))]
impl NativeOAuthPresenter {
    pub(crate) fn new(
        redirects: BridgeOAuthRedirects,
        callback: Box<dyn OAuthPresenterCallback>,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        Self { redirects, callback, diagnostics }
    }
}

#[cfg(any(test, all(feature = "oauth-providers", not(desktop))))]
#[async_trait::async_trait]
impl coven::OAuthPresenter for NativeOAuthPresenter {
    fn redirect_uri(&self, provider: coven::CloudProvider) -> &str {
        match provider {
            coven::CloudProvider::Dropbox => &self.redirects.dropbox,
            coven::CloudProvider::OneDrive => &self.redirects.onedrive,
            coven::CloudProvider::S3 => unreachable!("S3 uses access keys, never a sign-in"),
            coven::CloudProvider::CloudKit => unreachable!("CloudKit uses the system account"),
        }
    }

    async fn present(
        &self,
        authorization_url: &str,
    ) -> Result<coven::SecretText, coven::OAuthError> {
        crate::host_calls::host_call_async(
            |bug| self.diagnostics.panicked(bug),
            "present a native sign-in",
            self.callback.present(authorization_url.to_owned()),
        )
        .await
        .map_err(|failed| coven::OAuthError::Presentation(Box::new(failed)))?
        .map(coven::SecretText::new)
        .map_err(|error| match error {
            BridgeOAuthPresentationError::Cancelled => coven::OAuthError::Cancelled,
            error @ BridgeOAuthPresentationError::Failed { .. } => {
                coven::OAuthError::Presentation(Box::new(error))
            }
        })
    }
}

/// Public application identifiers. Dropbox uses a Full Dropbox registration.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOAuthClients {
    pub dropbox_app_key: Option<String>,
    pub onedrive_client_id: Option<String>,
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
