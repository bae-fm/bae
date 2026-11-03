use super::*;

/// The rates of a typical Mac output: built-in speakers, a display.
fn discrete(rates: &[u32]) -> Vec<RangeInclusive<u32>> {
    rates.iter().map(|rate| *rate..=*rate).collect()
}

#[test]
fn a_rate_the_device_runs_at_is_kept() {
    let device = discrete(&[44_100, 48_000, 88_200, 96_000]);
    assert_eq!(output_rate(44_100, &device), Some(44_100));
    assert_eq!(output_rate(96_000, &device), Some(96_000));
}

#[test]
fn a_higher_rate_converts_down_within_its_family() {
    let device = discrete(&[44_100, 48_000, 88_200, 96_000]);
    // 176.4 kHz goes to 88.2 kHz (2:1), not to the higher 96 kHz.
    assert_eq!(output_rate(176_400, &device), Some(88_200));
    // 192 kHz goes to 96 kHz (2:1), not to 88.2 kHz.
    assert_eq!(output_rate(192_000, &device), Some(96_000));
    // 352.8 kHz goes to the family's highest rate the device has.
    assert_eq!(output_rate(352_800, &device), Some(88_200));
}

#[test]
fn a_lower_rate_converts_up_to_the_lowest_of_its_family() {
    let device = discrete(&[44_100, 48_000, 88_200, 96_000]);
    assert_eq!(output_rate(22_050, &device), Some(44_100));
    assert_eq!(output_rate(32_000, &device), Some(48_000));
    assert_eq!(output_rate(16_000, &device), Some(48_000));
}

/// The family comes before the direction: a 44.1 kHz track on a device
/// with 32 kHz and 88.2 kHz goes up 1:2 to 88.2 kHz rather than down to
/// 32 kHz in the other family.
#[test]
fn the_family_comes_before_converting_down() {
    let device = discrete(&[32_000, 88_200]);
    assert_eq!(output_rate(44_100, &device), Some(88_200));
}

#[test]
fn a_device_without_the_tracks_family_converts_across() {
    // A 48 kHz-only device, as some USB and HDMI outputs are.
    let device = discrete(&[48_000]);
    assert_eq!(output_rate(44_100, &device), Some(48_000));
    assert_eq!(output_rate(176_400, &device), Some(48_000));
    // Of two rates in the other family, the highest at or below wins.
    let device = discrete(&[48_000, 96_000]);
    assert_eq!(output_rate(88_200, &device), Some(48_000));
}

#[test]
fn a_rate_of_no_family_uses_every_rate() {
    let device = discrete(&[44_100, 48_000]);
    assert_eq!(output_rate(47_250, &device), Some(44_100));
    assert_eq!(output_rate(50_400, &device), Some(48_000));
    assert_eq!(output_rate(37_800, &device), Some(44_100));
}

#[test]
fn a_continuous_range_offers_its_standard_rates() {
    // The track's rate inside the range is kept.
    let device = vec![8_000..=96_000];
    assert_eq!(output_rate(37_800, &device), Some(37_800));
    // Above the range: the family's highest standard rate inside it.
    assert_eq!(output_rate(176_400, &device), Some(88_200));
    assert_eq!(output_rate(192_000, &device), Some(96_000));
}

#[test]
fn a_device_with_no_rates_has_no_output_rate() {
    assert_eq!(output_rate(44_100, &[]), None);
}
