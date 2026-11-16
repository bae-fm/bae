use super::CopyResetNotice;
use bae_types::count_label::CountLabel;

fn counts(writes: u64, files: u64) -> Option<Vec<(String, u64)>> {
    CopyResetNotice::of(coven::UnsentWork { writes, files }).map(|notice| {
        notice.counts.into_iter().map(|CountLabel { key, count }| (key, count)).collect()
    })
}

/// When nothing is unsent, `CopyResetNotice::of` returns no notice, and the
/// notice leaves out a count of zero.
#[test]
fn the_notice_names_only_what_was_not_kept() {
    assert_eq!(counts(0, 0), None);
    assert_eq!(counts(2, 0), Some(vec![("core.sync.copy_reset.edits".to_string(), 2)]));
    assert_eq!(counts(0, 1), Some(vec![("core.sync.copy_reset.files".to_string(), 1)]));
    assert_eq!(
        counts(3, 1),
        Some(vec![
            ("core.sync.copy_reset.edits".to_string(), 3),
            ("core.sync.copy_reset.files".to_string(), 1),
        ])
    );
}

/// A count past what 32 bits hold keeps its value.
#[test]
fn the_notice_keeps_a_count_too_large_for_32_bits() {
    let many = u64::from(u32::MAX) + 1;
    assert_eq!(counts(many, 0), Some(vec![("core.sync.copy_reset.edits".to_string(), many)]));
}
