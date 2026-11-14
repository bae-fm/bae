use super::*;

#[test]
fn a_cost_line_carries_its_bar_for_the_suite_to_read() {
    assert_eq!(
        cost_line("album_page", Duration::from_micros(12_500), Some(50.0)),
        "COST metric=album_page value=12.5000 unit=ms bar=50"
    );
}

#[test]
fn a_cost_line_without_a_bar_has_none() {
    assert_eq!(
        cost_line("write/retitle", Duration::from_millis(3), None),
        "COST metric=write/retitle value=3.0000 unit=ms"
    );
}

#[test]
fn the_median_of_runs_is_the_middle_one() {
    let runs = [5, 1, 9, 3, 7].map(Duration::from_millis).to_vec();
    assert_eq!(median(runs), Duration::from_millis(5));
}
