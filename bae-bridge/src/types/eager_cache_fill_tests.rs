use super::BridgeEagerCacheFillStatus;

#[test]
fn eager_cache_fill_progress_crosses_the_bridge_without_losing_counters() {
    let status = BridgeEagerCacheFillStatus::from_core(coven::EagerCacheFillStatus::Downloading(
        coven::PinProgress {
            files_completed: 3,
            files_total: 8,
            bytes_cached: 12_345,
            bytes_total: 98_765,
        },
    ));

    assert_eq!(
        status,
        BridgeEagerCacheFillStatus::Downloading {
            progress: super::BridgeEagerCacheFillProgress {
                files_completed: 3,
                files_total: 8,
                bytes_cached: 12_345,
                bytes_total: 98_765,
            },
        }
    );
}
