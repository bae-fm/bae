use super::*;

/// A transport failure as reqwest reports it: a connection to a port nothing
/// listens on is refused.
async fn transport_error() -> DiscogsError {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let error = reqwest::Client::new()
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect_err("nothing listens on the port");
    DiscogsError::Transport(error)
}

#[test]
fn a_key_discogs_accepts_is_valid() {
    assert!(matches!(DiscogsCheck::of(Ok(())), Ok(DiscogsCheck::Valid)));
}

#[test]
fn a_key_discogs_turns_down_is_rejected() {
    assert!(matches!(
        DiscogsCheck::of(Err(DiscogsError::InvalidApiKey)),
        Ok(DiscogsCheck::Rejected)
    ));
}

/// A request that never reached Discogs says nothing about the key, and says
/// the person is offline.
#[tokio::test]
async fn a_request_that_never_reached_discogs_is_offline() {
    let check = DiscogsCheck::of(Err(transport_error().await));
    let Ok(DiscogsCheck::Offline(error)) = check else {
        panic!("a transport failure is the person being offline: {check:?}");
    };
    assert!(matches!(error, DiscogsError::Transport(_)), "{error}");
}

/// Discogs answering with something that is neither a verdict on the key nor
/// a lost connection is not offline: it stays an error, with its cause.
#[test]
fn an_answer_that_is_no_verdict_is_a_failure_not_offline() {
    for unusable in [
        DiscogsError::Provider {
            status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
            told_wait: None,
        },
        DiscogsError::NotFound,
        DiscogsError::Serialization(serde_json::from_str::<i32>("nope").unwrap_err()),
    ] {
        let reason = unusable.to_string();
        let Err(kept) = DiscogsCheck::of(Err(unusable)) else {
            panic!("{reason} is not a verdict on the key and not offline");
        };
        assert_eq!(kept.to_string(), reason);
    }
}
