#![cfg(feature = "test-utils")]
//! S3 setup validates credentials against the bucket *before* persisting.
//!
//! These tests require a minio (or any S3-compatible server) reachable at
//! `BAE_TEST_S3_URL` (default `http://localhost:19000`) with creds
//! `BAE_TEST_S3_KEY` / `BAE_TEST_S3_SECRET` (default `baetest`/`baetestpass`).
//! Marked `#[ignore]` so `cargo test` skips them; run with
//! `cargo test --test test_s3_setup_validation -- --ignored` when minio is up.

mod support;

use bae_core::config::CloudProvider;
use support::{setup_fresh_library, TestS3Endpoint};

#[test]
#[ignore]
fn save_s3_config_succeeds_against_a_real_bucket() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (lm, _tmp) = setup_fresh_library(&runtime);
    let s3 = TestS3Endpoint::from_env();
    let bucket = format!("bae-s3setup-ok-{}", uuid::Uuid::new_v4());
    runtime.block_on(s3.provision_bucket(&bucket));

    runtime
        .block_on(lm.save_s3_config(s3.config(&bucket, None)))
        .expect("save_s3_config should succeed");

    let config = lm.get_config();
    assert_eq!(config.cloud_home.provider, Some(CloudProvider::S3));
    assert_eq!(config.cloud_home.s3_bucket, Some(bucket));
    assert!(lm.is_sync_configured());
}

#[test]
#[ignore]
fn save_s3_config_fails_fast_for_missing_bucket() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (lm, _tmp) = setup_fresh_library(&runtime);
    let s3 = TestS3Endpoint::from_env();
    let bucket = format!("bae-s3setup-missing-{}", uuid::Uuid::new_v4());
    // Deliberately do NOT provision the bucket.

    let err = runtime
        .block_on(lm.save_s3_config(s3.config(&bucket, None)))
        .expect_err("save_s3_config should refuse a missing bucket");
    assert!(
        err.contains("does not exist") || err.contains("NoSuchBucket"),
        "expected bucket-doesn't-exist error, got: {err}",
    );

    // Crucially: state was not modified. Provider unset, sync not configured.
    let config = lm.get_config();
    assert_eq!(config.cloud_home.provider, None);
    assert!(!lm.is_sync_configured());
}

#[test]
#[ignore]
fn save_s3_config_fails_fast_for_bad_credentials() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (lm, _tmp) = setup_fresh_library(&runtime);
    let s3 = TestS3Endpoint::from_env();
    let bucket = format!("bae-s3setup-badcreds-{}", uuid::Uuid::new_v4());
    runtime.block_on(s3.provision_bucket(&bucket));

    let err = runtime
        .block_on(lm.save_s3_config(s3.config(&bucket, Some("wrong-secret"))))
        .expect_err("save_s3_config should refuse a bad secret");
    assert!(
        err.contains("rejected") || err.contains("SignatureDoesNotMatch"),
        "expected credentials-rejected error, got: {err}",
    );

    let config = lm.get_config();
    assert_eq!(config.cloud_home.provider, None);
    assert!(!lm.is_sync_configured());
}
