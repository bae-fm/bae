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

/// Redirects registered with the provider for the native app. Google admin
/// sign-in has none: it is a desktop admin source (`BridgeAdminAccess`), so
/// coven never asks a mobile presenter for its redirect.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOAuthRedirects {
    pub dropbox: String,
    pub onedrive: String,
}

#[cfg(any(
    test,
    all(
        feature = "oauth-providers",
        any(target_os = "ios", target_os = "android")
    )
))]
pub(crate) struct NativeOAuthPresenter {
    pub(crate) redirects: BridgeOAuthRedirects,
    pub(crate) callback: Box<dyn OAuthPresenterCallback>,
}

#[cfg(any(
    test,
    all(
        feature = "oauth-providers",
        any(target_os = "ios", target_os = "android")
    )
))]
#[async_trait::async_trait]
impl coven::OAuthPresenter for NativeOAuthPresenter {
    fn redirect_uri(&self, provider: coven::CloudProvider) -> &str {
        match provider {
            coven::CloudProvider::Dropbox => &self.redirects.dropbox,
            coven::CloudProvider::OneDrive => &self.redirects.onedrive,
            coven::CloudProvider::S3 => {
                unreachable!("Google admin sign-in is a desktop admin source")
            }
            coven::CloudProvider::CloudKit => unreachable!("CloudKit uses the system account"),
        }
    }

    async fn present(
        &self,
        authorization_url: &str,
    ) -> Result<coven::SecretText, coven::OAuthError> {
        self.callback
            .present(authorization_url.to_owned())
            .await
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
    pub google_admin_client_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use coven::OAuthPresenter;
    struct Cancelled;
    #[async_trait::async_trait]
    impl OAuthPresenterCallback for Cancelled {
        async fn present(&self, url: String) -> Result<String, BridgeOAuthPresentationError> {
            assert_eq!(url, "https://example.test/authorize");
            Err(BridgeOAuthPresentationError::Cancelled)
        }
    }
    #[tokio::test]
    async fn native_presentation_preserves_redirects_and_cancellation() {
        let presenter = NativeOAuthPresenter {
            redirects: BridgeOAuthRedirects {
                dropbox: "bae://dropbox".into(),
                onedrive: "bae://onedrive".into(),
            },
            callback: Box::new(Cancelled),
        };
        assert_eq!(
            presenter.redirect_uri(coven::CloudProvider::Dropbox),
            "bae://dropbox"
        );
        assert_eq!(
            presenter.redirect_uri(coven::CloudProvider::OneDrive),
            "bae://onedrive"
        );
        assert!(matches!(
            presenter.present("https://example.test/authorize").await,
            Err(coven::OAuthError::Cancelled)
        ));
    }
    struct Redirect;
    #[async_trait::async_trait]
    impl OAuthPresenterCallback for Redirect {
        async fn present(&self, _: String) -> Result<String, BridgeOAuthPresentationError> {
            Ok("bae://dropbox?code=authorization-code&state=request-state".into())
        }
    }
    #[tokio::test]
    async fn native_presentation_returns_the_complete_redirect_to_coven() {
        let presenter = NativeOAuthPresenter {
            redirects: BridgeOAuthRedirects {
                dropbox: "bae://dropbox".into(),
                onedrive: "bae://onedrive".into(),
            },
            callback: Box::new(Redirect),
        };
        let redirect = presenter
            .present("https://example.test/authorize")
            .await
            .unwrap();
        assert_eq!(
            redirect.as_str(),
            "bae://dropbox?code=authorization-code&state=request-state"
        );
    }
}
