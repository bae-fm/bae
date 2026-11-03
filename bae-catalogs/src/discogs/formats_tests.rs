use super::*;

fn format(name: &str, qty: &str, descriptions: &[&str]) -> DiscogsFormat {
    DiscogsFormat {
        name: name.to_string(),
        qty: qty.to_string(),
        descriptions: descriptions.iter().map(|d| d.to_string()).collect(),
        text: None,
    }
}

/// A box of records modelled on a real one: two single LPs, a four-LP
/// set and the box they are sold in.
#[test]
fn a_boxed_set_reads_as_its_records_its_box_and_its_details() {
    let reading = read(
        "1",
        &[
            format("Vinyl", "1", &["LP", "Album", "Reissue", "Remastered"]),
            format("Vinyl", "1", &["LP", "Album", "Reissue", "Remastered"]),
            format("Vinyl", "4", &["LP", "Album"]),
            format("Box Set", "1", &["Compilation", "Deluxe Edition"]),
        ],
    );
    assert_eq!(
        reading.media.counts(),
        vec![bae_types::pressing::MediaCount { medium: Medium::Vinyl, count: 6 }]
    );
    assert_eq!(reading.packaging, Some(Packaging::Box));
    assert_eq!(reading.status, None);
    assert_eq!(
        reading.details,
        vec![
            DiscogsDetail::Lp,
            DiscogsDetail::Reissue,
            DiscogsDetail::Remastered,
            DiscogsDetail::DeluxeEdition,
        ],
        "release types are dropped and each detail is kept once"
    );
}

#[test]
fn a_promotional_copy_states_its_status() {
    let reading = read("1", &[format("CD", "1", &["Album", "Promo", "Stereo"])]);
    assert_eq!(reading.status, Some(ReleaseStatus::Promotion));
    assert_eq!(reading.details, vec![DiscogsDetail::Stereo]);
}

#[test]
fn an_unofficial_promotional_copy_is_a_bootleg() {
    let reading = read("1", &[format("Vinyl", "1", &["12\"", "Promo", "Unofficial Release"])]);
    assert_eq!(reading.status, Some(ReleaseStatus::Bootleg));
}

#[test]
fn a_hybrid_disc_is_the_kind_its_description_names() {
    let reading = read("1", &[format("Hybrid", "1", &["DualDisc", "Album"])]);
    assert_eq!(
        reading.media,
        StatedMedia::Formats(vec![StatedFormat { medium: Some(Medium::DualDisc), quantity: 1 }])
    );
    let unstated = read("1", &[format("Hybrid", "1", &["Album"])]);
    assert_eq!(
        unstated.media,
        StatedMedia::Formats(vec![StatedFormat { medium: None, quantity: 1 }])
    );
}

#[test]
fn words_outside_the_lists_are_left_out() {
    let reading = read("1", &[format("Zorblax", "0", &["Reissue", "180 Gram"])]);
    assert_eq!(
        reading.media,
        StatedMedia::Formats(vec![StatedFormat { medium: None, quantity: 1 }])
    );
    assert_eq!(reading.details, vec![DiscogsDetail::Reissue]);
    assert_eq!(read("1", &[]).media, StatedMedia::Undescribed);
}
