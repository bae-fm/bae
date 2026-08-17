use super::BridgeEagerCacheFillStatus;

#[test]
fn eager_cache_fill_progress_crosses_the_bridge_without_losing_counters() {
    let status = BridgeEagerCacheFillStatus::from_core(coven::EagerCacheFillStatus::Downloading(
        coven::EagerCacheFillProgress {
            files_done: 3,
            files_total: 8,
            bytes_done: 12_345,
            bytes_total: 98_765,
        },
    ));

    assert_eq!(
        status,
        BridgeEagerCacheFillStatus::Downloading {
            title_key: "core.artwork_cache.downloading".to_string(),
            progress: super::BridgeEagerCacheFillProgress {
                files_done: 3,
                files_total: 8,
                bytes_done: 12_345,
                bytes_total: 98_765,
            },
        }
    );
}
