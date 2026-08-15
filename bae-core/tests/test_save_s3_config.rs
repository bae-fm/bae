#![cfg(feature = "test-utils")]
//! `save_s3_config` asks Coven to prepare the provider and persists the returned
//! config only after Coven installs the connection. A failed probe therefore
//! leaves bae's config unchanged; Coven's own setup tests cover custody rollback.
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

fn unreachable_s3_config() -> S3ConfigData {
    S3ConfigData {
        bucket: "any-bucket".to_string(),
        region: "us-east-1".to_string(),
        endpoint: Some("http://127.0.0.1:1".to_string()),
        key_prefix: None,
        access_key: "any-key".to_string(),
        secret_key: "any-secret".to_string(),
        storage: HomeStorage::Opaque,
    }
}

#[test]
fn probe_failure_persists_nothing_and_surfaces_as_cloud_setup_network() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (lm, _tmp) = setup_fresh_library(&runtime);

    // Nothing listens on port 1, so the probe fails with connection-refused
    // before `save_s3_config` reaches any of its persisting steps.
    let err = runtime
        .block_on(lm.save_s3_config(unreachable_s3_config()))
        .expect_err("an unreachable endpoint must fail the probe");
    // An unreachable backend is the network case within the cloud-setup
    // category, distinct from the authentication and permission cases.
    assert_eq!(
        err.category(),
        UiErrorCategory::CloudSetup(coven::CloudHomeSetupFailure::Network),
        "got: {err}"
    );

    // The library is untouched: no provider recorded and sync is unconfigured.
    let config = lm.get_config();
    assert_eq!(config.cloud_home.provider, None);
    assert!(!lm.is_sync_configured());
}

#[test]
fn save_s3_config_survives_a_narrow_host_stack() {
    const CHILD: &str = "BAE_S3_NARROW_STACK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let runtime = tokio::runtime::Runtime::new().expect("build test runtime");
        let (manager, _library) = setup_fresh_library(&runtime);
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("narrow-s3-host".to_string())
                .stack_size(512 * 1024)
                .spawn_scoped(scope, move || {
                    runtime
                        .block_on(manager.save_s3_config(unreachable_s3_config()))
                        .expect_err("an unreachable endpoint must fail the probe");
                })
                .expect("spawn narrow S3 host")
                .join()
                .expect("narrow S3 host completes");
        });
        return;
    }

    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .arg("save_s3_config_survives_a_narrow_host_stack")
        .arg("--nocapture")
        .env(CHILD, "1")
        .status()
        .expect("run narrow-stack S3 subprocess");
    assert!(
        status.success(),
        "saving S3 configuration overflowed its host stack: {status}"
    );
}
