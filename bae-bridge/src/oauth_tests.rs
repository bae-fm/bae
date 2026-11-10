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
    let presenter = NativeOAuthPresenter::new(
        BridgeOAuthRedirects { dropbox: "bae://dropbox".into(), onedrive: "bae://onedrive".into() },
        Box::new(Cancelled),
        bae_diagnostics::Diagnostics::noop(),
    );
    assert_eq!(presenter.redirect_uri(coven::CloudProvider::Dropbox), "bae://dropbox");
    assert_eq!(presenter.redirect_uri(coven::CloudProvider::OneDrive), "bae://onedrive");
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
    let presenter = NativeOAuthPresenter::new(
        BridgeOAuthRedirects { dropbox: "bae://dropbox".into(), onedrive: "bae://onedrive".into() },
        Box::new(Redirect),
        bae_diagnostics::Diagnostics::noop(),
    );
    let redirect = presenter.present("https://example.test/authorize").await.unwrap();
    assert_eq!(redirect.as_str(), "bae://dropbox?code=authorization-code&state=request-state");
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
/// The throw fails the sign-in with a presentation error, instead of
/// unwinding through coven. uniffi raises it as a panic, which the panic
/// hook reported, so the sign-in doesn't report it again.
#[tokio::test]
async fn a_sign_in_sheet_that_throws_fails_the_sign_in() {
    let bugs = std::sync::Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let presenter = NativeOAuthPresenter::new(
        BridgeOAuthRedirects { dropbox: "bae://dropbox".into(), onedrive: "bae://onedrive".into() },
        Box::new(Throwing),
        bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    );
    assert!(matches!(
        presenter.present("https://example.test/authorize").await,
        Err(coven::OAuthError::Presentation(_))
    ));
    assert_eq!(bugs.reports(), Vec::new());
}
