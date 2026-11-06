//! A fake Dropbox sign-in the integration tests sign bae in through.

/// Signs bae in to a fake Dropbox. The presenter answers every authorization
/// with a code, and the token endpoint grants `access-N` for the Nth code. It
/// refuses every refresh, so a token Dropbox rejects needs a new sign-in.
pub(super) struct DropboxSignIn {
    endpoint: String,
    presented: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}

impl DropboxSignIn {
    pub(super) async fn start() -> Self {
        use axum::{http::StatusCode, Form, Json};
        let granted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let router = axum::Router::new().fallback(
            move |Form(form): Form<std::collections::HashMap<String, String>>| {
                let granted = granted.clone();
                async move {
                    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({ "error": "invalid_grant" })),
                        );
                    }
                    let n = granted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "access_token": format!("access-{n}"),
                            "refresh_token": format!("refresh-{n}"),
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })),
                    )
                }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { endpoint, presented: std::sync::Arc::default(), server }
    }

    pub(super) fn configure(&self, builder: coven::CovenBuilder) -> coven::CovenBuilder {
        builder
            .oauth_clients(
                coven::OAuthClients::new(
                    Some(coven::DropboxClients::FullDropbox { app_key: "dropbox-key".into() }),
                    None,
                )
                .with_token_endpoint(self.endpoint.clone()),
            )
            .oauth_presenter(std::sync::Arc::new(CodePresenter {
                presented: self.presented.clone(),
            }))
    }

    /// How many sign-ins bae has presented.
    pub(super) fn presented(&self) -> usize {
        self.presented.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for DropboxSignIn {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// Answers each authorization with a code for its own state, as the person
/// approving it in Dropbox's sheet would.
struct CodePresenter {
    presented: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait::async_trait]
impl coven::OAuthPresenter for CodePresenter {
    fn redirect_uri(&self, _: coven::CloudProvider) -> &str {
        "bae-test:/oauth"
    }

    async fn present(
        &self,
        authorization_url: &str,
    ) -> Result<coven::SecretText, coven::OAuthError> {
        self.presented.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let url = url::Url::parse(authorization_url).unwrap();
        let (_, state) = url.query_pairs().find(|(name, _)| name == "state").unwrap();
        Ok(coven::SecretText::new(format!("bae-test:/oauth?code=approved&state={state}")))
    }
}
