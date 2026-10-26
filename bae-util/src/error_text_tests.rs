use super::*;

/// A cause below another, as reqwest and hyper nest them.
#[derive(Debug)]
struct Layer(&'static str, Option<Box<Layer>>);

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Error for Layer {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.1.as_deref().map(|layer| layer as _)
    }
}

/// Every layer prints, in order, and each URL keeps only its scheme, host and
/// port: a path, a query and credentials never print.
#[test]
fn every_cause_prints_with_its_urls_cut_to_the_host() {
    let error = Layer(
        "error sending request for url (https://user:secret@api.example:8443/ws/2/release/rel-123?inc=labels&token=deadbeef)",
        Some(Box::new(Layer(
            "client error (Connect)",
            Some(Box::new(Layer("dns error: no such host", None))),
        ))),
    );
    let printed = CauseChain(&error).to_string();
    assert_eq!(
        printed,
        "error sending request for url (https://api.example:8443/…): \
         client error (Connect): dns error: no such host"
    );
    for hidden in ["secret", "user", "rel-123", "deadbeef", "token"] {
        assert!(!printed.contains(hidden), "{hidden} in {printed}");
    }
}

#[test]
fn text_that_is_not_a_url_is_kept_and_a_malformed_one_is_hidden() {
    assert_eq!(redact_urls("no url here"), "no url here");
    assert_eq!(
        redact_urls("at http://[bad and https://ok.example/x"),
        "at <url> and https://ok.example/…"
    );
}

/// A request to a host no resolver knows names the DNS failure, which
/// reqwest's own text leaves to a cause below it.
#[tokio::test]
async fn an_unresolvable_host_prints_its_dns_failure() {
    let error = reqwest::Client::new()
        .get("http://bae-test.invalid/path?secret=1")
        .send()
        .await
        .expect_err("a .invalid host never resolves");
    let printed = CauseChain(&error).to_string();
    assert!(!error.to_string().contains("dns error"), "{error}");
    assert!(printed.contains("dns error"), "{printed}");
    assert!(!printed.contains("secret"), "{printed}");
}

/// A request to a port nothing listens on names the refused connection.
#[tokio::test]
async fn a_refused_connection_prints_its_connect_failure() {
    let error = crate::http::refused_request_error().await;
    let printed = CauseChain(&error).to_string();
    assert!(printed.contains("tcp connect error"), "{printed}");
}
