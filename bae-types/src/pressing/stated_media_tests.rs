use super::*;

#[test]
fn counts_add_up_each_carrier_in_first_appearance_order() {
    let per_medium =
        StatedMedia::PerMedium(vec![Some(Medium::Cd), None, Some(Medium::Dvd), Some(Medium::Cd)]);
    assert_eq!(
        per_medium.counts(),
        vec![
            MediaCount { medium: Medium::Cd, count: 2 },
            MediaCount { medium: Medium::Dvd, count: 1 },
        ]
    );
    let formats = StatedMedia::Formats(vec![
        StatedFormat { medium: Some(Medium::Vinyl), quantity: 1 },
        StatedFormat { medium: None, quantity: 1 },
        StatedFormat { medium: Some(Medium::Vinyl), quantity: 3 },
    ]);
    assert_eq!(formats.counts(), vec![MediaCount { medium: Medium::Vinyl, count: 4 }]);
    assert!(StatedMedia::Undescribed.counts().is_empty());
}
