use super::*;

/// Fixed TOCs with their MusicBrainz DiscIDs. The expected strings were
/// produced by the reference C library (`discid` crate) and baked in as
/// literals so these tests outlive the dependency. `offsets[0]` is the
/// lead-out; the rest are track start sectors including the 150-sector pregap.
fn fixed_vectors() -> Vec<(Vec<i32>, &'static str)> {
    vec![
        // 11-track disc — the `discid` crate's own documented example.
        (
            vec![
                242457, 150, 44942, 61305, 72755, 96360, 130485, 147315, 164275, 190702, 205412,
                220437,
            ],
            "lSOVc5h6IXSuzcamJS1Gp4_tRuA-",
        ),
        // Single track, lead-out after the 150-sector pregap.
        (vec![180_150, 150], "NyB4TAo0fl6yU3tBAcL.jKphZs0-"),
        // Typical 3-track disc.
        (vec![270_150, 150, 90_150, 180_150], "OW7Dlo_1oOnvIk1X5vG229avEII-"),
        // 99 tracks — the maximum a CD (and this hash) allows.
        (max_track_toc(), "qOt.RNFgr441aSUXpEjVegEMurg-"),
    ]
}

/// A 99-track TOC: track k starts at `150 + k*1000`, lead-out after track 99.
fn max_track_toc() -> Vec<i32> {
    let mut offsets = vec![150 + 100 * 1000];
    for k in 0..99 {
        offsets.push(150 + k * 1000);
    }
    offsets
}

#[test]
fn known_musicbrainz_discids() {
    for (offsets, expected) in fixed_vectors() {
        assert_eq!(
            musicbrainz_discid(&offsets).expect("valid TOC"),
            expected,
            "DiscID mismatch for offsets {offsets:?}"
        );
    }
}

#[test]
fn discid_is_28_url_safe_chars() {
    let id = musicbrainz_discid(&fixed_vectors()[0].0).unwrap();
    assert_eq!(id.len(), 28);
    assert!(id.ends_with('-'));
    assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')));
}

#[test]
fn rejects_no_tracks() {
    assert_eq!(musicbrainz_discid(&[150]), Err(DiscIdError::IllegalTrackCount(0)));
}

#[test]
fn rejects_too_many_tracks() {
    let offsets = vec![0; 102]; // lead-out + 101 tracks
    assert_eq!(musicbrainz_discid(&offsets), Err(DiscIdError::IllegalTrackCount(101)));
}

#[test]
fn rejects_disc_too_long() {
    let lead_out = MAX_DISC_LENGTH + 1;
    assert_eq!(musicbrainz_discid(&[lead_out, 150]), Err(DiscIdError::DiscTooLong(lead_out)));
}

#[test]
fn rejects_track_past_lead_out() {
    assert_eq!(
        musicbrainz_discid(&[1000, 2000]),
        Err(DiscIdError::OffsetPastLeadOut { index: 1, offset: 2000, lead_out: 1000 })
    );
}

#[test]
fn rejects_out_of_order_tracks() {
    assert_eq!(
        musicbrainz_discid(&[10_000, 150, 5000, 4000]),
        Err(DiscIdError::OffsetsOutOfOrder { index: 3 })
    );
}
