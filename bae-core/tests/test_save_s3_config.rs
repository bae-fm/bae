#![cfg(feature = "test-utils")]
//! `save_s3_config` probes the bucket with the proposed credentials *before* it
//! writes anything durable — the config's provider, the keyring credentials, the
//! running sync manager. So a probe that fails leaves the library exactly as it
//! was: no half-configured cloud home the UI would show as "Connected" while the
//! first sync cycle silently fails.
//!
//! Hermetic: pointing the probe at an address nothing listens on
//! (`127.0.0.1:1`) fails the probe with connection-refused — no server, no
//! `#[ignore]`, no env. The live-bucket probe outcomes (missing bucket, bad
//! secret) are coven's own `S3CloudHome::probe` tests; bae's contribution is the
//! atomicity asserted here.

use bae_test_support as support;

use bae_core::config::HomeStorage;
use bae_core::sync::S3ConfigData;
use bae_core::ui::UiErrorCategory;
use support::setup_fresh_library;

#[test]
fn probe_failure_persists_nothing_and_surfaces_as_network() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (lm, _tmp) = setup_fresh_library(&runtime);

    // Nothing listens on port 1, so the probe fails with connection-refused
    // before `save_s3_config` reaches any of its persisting steps.
    let unreachable = S3ConfigData {
        bucket: "any-bucket".to_string(),
        region: "us-east-1".to_string(),
        endpoint: Some("http://127.0.0.1:1".to_string()),
        key_prefix: None,
        access_key: "any-key".to_string(),
        secret_key: "any-secret".to_string(),
        storage: HomeStorage::Opaque,
    };

    let err = runtime
        .block_on(lm.save_s3_config(unreachable))
        .expect_err("an unreachable endpoint must fail the probe");
    // An unreachable backend is the transient network class the UI offers a
    // retry for — distinct from the credentials class a rejected bucket/secret
    // maps to.
    assert_eq!(err.category(), UiErrorCategory::Network, "got: {err}");

    // The probe ran before any durable write, so the library is untouched: no
    // provider recorded, sync still unconfigured.
    let config = lm.get_config();
    assert_eq!(config.cloud_home.provider, None);
    assert!(!lm.is_sync_configured());
}
