use super::*;
use bae_types::pressing::StatedFormat;

fn track(position: &str) -> DiscogsTrack {
    DiscogsTrack {
        position: position.to_string(),
        title: format!("Track Title {position}"),
        duration: None,
        artists: vec![],
        extraartists: None,
        type_: "track".to_string(),
        sub_tracks: vec![],
    }
}

fn heading(title: &str) -> DiscogsTrack {
    DiscogsTrack {
        position: String::new(),
        title: title.to_string(),
        duration: None,
        artists: vec![],
        extraartists: None,
        type_: "heading".to_string(),
        sub_tracks: vec![],
    }
}

fn index(position: &str, sub_tracks: Vec<DiscogsTrack>) -> DiscogsTrack {
    DiscogsTrack {
        position: position.to_string(),
        title: "Suite Title".to_string(),
        duration: None,
        artists: vec![],
        extraartists: None,
        type_: "index".to_string(),
        sub_tracks,
    }
}

/// `count` rows `<prefix>1` … `<prefix><count>`.
fn rows(prefix: &str, count: u32) -> Vec<DiscogsTrack> {
    (1..=count)
        .map(|n| track(&format!("{prefix}{n}")))
        .collect()
}

fn formats(entries: &[(Medium, u32)]) -> StatedMedia {
    StatedMedia::Formats(
        entries
            .iter()
            .map(|(medium, quantity)| StatedFormat {
                medium: Some(*medium),
                quantity: *quantity,
            })
            .collect(),
    )
}

/// Each medium's carrier and the positions of its rows.
fn shape(mediums: &[MediumRows]) -> Vec<(Option<Medium>, Vec<String>)> {
    mediums
        .iter()
        .map(|medium| {
            (
                medium.medium,
                medium.rows.iter().map(|row| row.position.clone()).collect(),
            )
        })
        .collect()
}

fn positions(rows: &[DiscogsTrack]) -> Vec<String> {
    rows.iter().map(|row| row.position.clone()).collect()
}

/// A box of two LPs, two CDs and a Blu-ray, listed as Discogs lists one: a
/// heading over the records' sides, a heading over the CDs' prefixed rows,
/// a heading over the Blu-ray's one row.
#[test]
fn a_mixed_box_splits_into_each_record_disc_and_video_with_its_carrier() {
    let mut tracklist = vec![heading("The Records")];
    for side in ["A", "B"] {
        tracklist.extend(rows(side, 6));
    }
    for side in ["C", "D"] {
        tracklist.extend(rows(side, 5));
    }
    tracklist.push(heading("The Compact Discs"));
    tracklist.extend(rows("CD1-", 12));
    tracklist.extend(rows("CD2-", 10));
    tracklist.push(heading("The Video"));
    tracklist.push(track("BD-1"));
    let media = formats(&[
        (Medium::Vinyl, 2),
        (Medium::Cd, 1),
        (Medium::Cd, 1),
        (Medium::BluRay, 1),
    ]);

    let mediums = split("1", &tracklist, &media);

    let mut first_record = vec![String::new()];
    first_record.extend(positions(&rows("A", 6)));
    first_record.extend(positions(&rows("B", 6)));
    let mut second_record = positions(&rows("C", 5));
    second_record.extend(positions(&rows("D", 5)));
    let mut first_cd = vec![String::new()];
    first_cd.extend(positions(&rows("CD1-", 12)));
    assert_eq!(
        shape(&mediums),
        vec![
            (Some(Medium::Vinyl), first_record),
            (Some(Medium::Vinyl), second_record),
            (Some(Medium::Cd), first_cd),
            (Some(Medium::Cd), positions(&rows("CD2-", 10))),
            (
                Some(Medium::BluRay),
                vec![String::new(), "BD-1".to_string()]
            ),
        ],
        "each heading goes with the medium of the row after it"
    );
}

/// A box whose every record numbers its sides from A again is one record per
/// restart, not one record of repeated sides.
#[test]
fn a_box_that_restarts_at_side_a_splits_per_record() {
    let mut tracklist = Vec::new();
    for _ in 0..3 {
        tracklist.extend(rows("A", 2));
        tracklist.extend(rows("B", 2));
    }

    let mediums = split("1", &tracklist, &formats(&[(Medium::Vinyl, 3)]));

    let record = vec!["A1", "A2", "B1", "B2"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    assert_eq!(
        shape(&mediums),
        vec![
            (Some(Medium::Vinyl), record.clone()),
            (Some(Medium::Vinyl), record.clone()),
            (Some(Medium::Vinyl), record),
        ]
    );
}

#[test]
fn numbered_discs_take_the_formats_in_order() {
    let mut tracklist = rows("1-", 5);
    tracklist.extend(rows("2-", 5));

    let two_cds = split("1", &tracklist, &formats(&[(Medium::Cd, 2)]));
    assert_eq!(
        shape(&two_cds),
        vec![
            (Some(Medium::Cd), positions(&rows("1-", 5))),
            (Some(Medium::Cd), positions(&rows("2-", 5))),
        ]
    );

    let cd_and_dvd = split(
        "1",
        &tracklist,
        &formats(&[(Medium::Cd, 1), (Medium::Dvd, 1)]),
    );
    assert_eq!(
        cd_and_dvd
            .iter()
            .map(|medium| medium.medium)
            .collect::<Vec<_>>(),
        vec![Some(Medium::Cd), Some(Medium::Dvd)]
    );
}

/// Formats that count fewer media than the positions number still say the
/// carrier when they name one; naming several, they say none.
#[test]
fn a_miscounted_format_names_the_carrier_only_when_it_is_the_only_one() {
    let mut discs = rows("1-", 2);
    discs.extend(rows("2-", 2));
    discs.extend(rows("3-", 2));
    let one_carrier = split("1", &discs, &formats(&[(Medium::Cd, 1)]));
    assert_eq!(
        one_carrier
            .iter()
            .map(|medium| medium.medium)
            .collect::<Vec<_>>(),
        vec![Some(Medium::Cd); 3]
    );
    let two_carriers = split("1", &discs, &formats(&[(Medium::Cd, 1), (Medium::Dvd, 1)]));
    assert_eq!(
        two_carriers
            .iter()
            .map(|medium| medium.medium)
            .collect::<Vec<_>>(),
        vec![None; 3]
    );

    let mut sides = Vec::new();
    for side in ["A", "B", "C", "D"] {
        sides.extend(rows(side, 2));
    }
    let records = split("1", &sides, &formats(&[(Medium::Vinyl, 1)]));
    assert_eq!(
        records
            .iter()
            .map(|medium| medium.medium)
            .collect::<Vec<_>>(),
        vec![Some(Medium::Vinyl); 2],
        "four sides are two records"
    );
}

#[test]
fn a_tracklist_numbering_no_medium_is_one_medium() {
    let tracklist = rows("", 10);
    let mediums = split("1", &tracklist, &formats(&[(Medium::Cd, 1)]));
    assert_eq!(
        shape(&mediums),
        vec![(Some(Medium::Cd), positions(&tracklist))]
    );

    let unstated = split("1", &tracklist, &StatedMedia::Undescribed);
    assert_eq!(shape(&unstated), vec![(None, positions(&tracklist))]);
}

/// A record whose positions print no side letters is still the record the
/// formats name.
#[test]
fn an_unlettered_record_is_the_record_its_format_names() {
    let tracklist = rows("", 6);
    let mediums = split("1", &tracklist, &formats(&[(Medium::Vinyl, 1)]));
    assert_eq!(mediums.len(), 1);
    assert_eq!(mediums[0].medium, Some(Medium::Vinyl));
}

#[test]
fn an_empty_tracklist_has_no_mediums() {
    assert!(split("1", &[], &formats(&[(Medium::Cd, 1)])).is_empty());
}

/// A heading or an index with no position goes with the disc its rows are
/// on: a heading with the next numbered row, an index with its sub-tracks,
/// and anything after the last numbered row with the last disc.
#[test]
fn rows_naming_no_disc_go_with_their_neighbours() {
    let mut tracklist = rows("1-", 9);
    tracklist.push(index(
        "",
        vec![track("1-10a"), track("1-10b"), track("1-10c")],
    ));
    tracklist.push(heading("Second Disc"));
    tracklist.extend(rows("2-", 10));
    tracklist.push(heading("Trailing Heading"));

    let mediums = split("1", &tracklist, &formats(&[(Medium::Cd, 2)]));

    let mut first = positions(&rows("1-", 9));
    first.push(String::new());
    let mut second = vec![String::new()];
    second.extend(positions(&rows("2-", 10)));
    second.push(String::new());
    assert_eq!(
        shape(&mediums),
        vec![(Some(Medium::Cd), first), (Some(Medium::Cd), second)]
    );
}

#[test]
fn positions_read_by_the_grammar() {
    assert_eq!(place("2-5"), Some(Place::Disc(2)));
    assert_eq!(place("1-10a"), Some(Place::Disc(1)));
    assert_eq!(
        place("CD2-3"),
        Some(Place::Prefixed {
            medium: Medium::Cd,
            disc: Some(2)
        })
    );
    assert_eq!(
        place("BD-1"),
        Some(Place::Prefixed {
            medium: Medium::BluRay,
            disc: None
        })
    );
    assert_eq!(
        place("dvd4"),
        Some(Place::Prefixed {
            medium: Medium::Dvd,
            disc: None
        })
    );
    assert_eq!(place("B1ii"), Some(Place::Side(1)));
    assert_eq!(place("A"), Some(Place::Side(0)));
    assert_eq!(place("7"), None, "a plain number names no medium");
    assert_eq!(place("XY1-1"), None, "an unknown prefix names no medium");
    assert_eq!(place("0-1"), None, "discs count from one");
    assert_eq!(place(""), None);
}
