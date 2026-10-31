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
    pub(crate) redirects: BridgeOAuthRedirects,
    pub(crate) callback: Box<dyn OAuthPresenterCallback>,
    /// Where a throw the callback doesn't declare is reported.
    pub(crate) diagnostics: std::sync::Arc<crate::BridgeDiagnostics>,
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
            |failed| self.diagnostics.bug(failed),
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
            diagnostics: crate::BridgeDiagnostics::noop(),
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
            diagnostics: crate::BridgeDiagnostics::noop(),
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

    /// A sign-in sheet whose host throws something its callback doesn't
    /// declare, which uniffi raises in Rust as a panic.
    struct Throwing;
    #[async_trait::async_trait]
    impl OAuthPresenterCallback for Throwing {
        async fn present(&self, _: String) -> Result<String, BridgeOAuthPresentationError> {
            panic!("Callback interface failure: the host threw");
        }
    }
    /// The throw fails the sign-in with a presentation error and is
    /// reported, instead of unwinding through coven.
    #[tokio::test]
    async fn a_sign_in_sheet_that_throws_fails_the_sign_in_and_is_reported() {
        let bugs = std::sync::Arc::new(bae_core::diagnostics::RecordingBugReporter::default());
        let presenter = NativeOAuthPresenter {
            redirects: BridgeOAuthRedirects {
                dropbox: "bae://dropbox".into(),
                onedrive: "bae://onedrive".into(),
            },
            callback: Box::new(Throwing),
            diagnostics: crate::BridgeDiagnostics::reporting_bugs_to(bugs.clone()),
        };
        assert!(matches!(
            presenter.present("https://example.test/authorize").await,
            Err(coven::OAuthError::Presentation(_))
        ));
        let reports = bugs.reports();
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0].0, "present a native sign-in");
    }
}
