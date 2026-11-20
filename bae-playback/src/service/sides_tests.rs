use super::*;
use crate::service::tests::*;
use bae_types::mediums::{SideHeading, TrackSide};

fn test_track_info(track_id: &str) -> PlaybackTrackInfo {
    PlaybackTrackInfo {
        track_id: track_id.to_string(),
        release_id: "release-id".to_string(),
        side: None,
    }
}

fn sided(letter: &str) -> TrackSide {
    TrackSide::Sided { side_letter: letter.to_string() }
}

/// Two tracks of one release on physical carriers, the first on the side or
/// disc `last` and the second on `next`.
fn tracks_across(last: TrackSide, next: TrackSide) -> (PlaybackTrackInfo, PlaybackTrackInfo) {
    let on = |track_id: &str, side: TrackSide| PlaybackTrackInfo {
        side: Some(side),
        ..test_track_info(track_id)
    };
    (on("last", last), on("next", next))
}

/// A record's side ends at the next side, and a CD at the next disc; the
/// prompt carries the side or disc that ended.
#[test]
fn a_pause_carries_the_side_that_ended() {
    for (last, next) in [
        (sided("B"), sided("C")),
        (sided("A"), sided("B")),
        (TrackSide::Disc { disc: 2 }, TrackSide::Disc { disc: 3 }),
    ] {
        let heading = last.heading();
        let (last_track, next_track) = tracks_across(last, next);
        let boundary = side_boundary_between(&last_track, &next_track).expect("the side ends");
        assert_eq!(Some(boundary.ended.clone()), heading);
        let prompt =
            SidePauseDecision { track_id: "next".to_string(), boundary, resumes_at: None }.prompt();
        assert_eq!(Some(prompt.ended), heading);
    }
}

/// Two tracks under one heading play on, and a flat side, which has no
/// heading to name, never ends in a pause.
#[test]
fn no_pause_under_one_heading_or_after_a_flat_side() {
    let (first, second) = tracks_across(sided("A"), sided("A"));
    assert_eq!(side_boundary_between(&first, &second), None);
    let (flat, disc) = tracks_across(TrackSide::Flat, TrackSide::Disc { disc: 2 });
    assert_eq!(side_boundary_between(&flat, &disc), None);
}

#[test]
fn side_pause_prompt_carries_its_countdown() {
    let decision = SidePauseDecision {
        track_id: "next".to_string(),
        boundary: SideBoundary {
            id: "next:disc 1".to_string(),
            ended: SideHeading::Disc { disc: 1 },
        },
        resumes_at: Some(countdown_deadline()),
    };

    assert_eq!(
        decision.prompt(),
        PlaybackSidePausePrompt {
            id: "next:disc 1".to_string(),
            ended: SideHeading::Disc { disc: 1 },
            countdown: Some(PlaybackSideCountdown { resumes_at: countdown_deadline() }),
        }
    );
    assert_eq!(SidePauseDecision { resumes_at: None, ..decision }.prompt().countdown, None);
}

/// What playback reads of a track on medium `medium` of a release of
/// `mediums`, on the side or disc `on`.
fn info_on(
    track_id: &str,
    mediums: &bae_records::DbReleaseMediums,
    medium: usize,
    on: bae_types::mediums::TrackSide,
) -> PlaybackTrackInfo {
    PlaybackTrackInfo::of(bae_records::DbTrackPlace {
        track_id: track_id.to_string(),
        release_id: "release-id".to_string(),
        carrier: mediums.0[medium - 1].medium.carrier,
        on,
    })
}

fn release_of(mediums: &[bae_types::mediums::ReleaseMedium]) -> bae_records::DbReleaseMediums {
    bae_records::DbReleaseMediums(
        mediums
            .iter()
            .enumerate()
            .map(|(index, medium)| {
                bae_records::DbReleaseMedium::new(
                    &coven::UuidIds,
                    "release-id",
                    bae_types::mediums::medium_position(index),
                    *medium,
                    time::UtcDateTime::now(),
                )
            })
            .collect(),
    )
}

/// A CD release pauses where its discs change and nowhere else, the pause
/// named by the disc that ended.
#[test]
fn a_cd_release_pauses_at_disc_changes_only() {
    let cd = bae_types::mediums::ReleaseMedium::one_sided(Some(bae_types::pressing::Medium::Cd));
    let mediums = release_of(&[cd, cd]);
    let disc = |disc| bae_types::mediums::TrackSide::Disc { disc };
    let first = info_on("first", &mediums, 1, disc(1));
    let second = info_on("second", &mediums, 1, disc(1));
    let third = info_on("third", &mediums, 2, disc(2));
    assert_eq!(side_boundary_between(&first, &second), None);
    let boundary = side_boundary_between(&second, &third).expect("disc 1 ends");
    assert_eq!(boundary.ended, SideHeading::Disc { disc: 1 });
}

/// A 2×LP rip pauses at each side, named by its letter across both records.
#[test]
fn a_two_record_release_pauses_at_each_side() {
    let record = bae_types::mediums::ReleaseMedium {
        carrier: Some(bae_types::pressing::Medium::Vinyl),
        sides: std::num::NonZeroU8::new(2).expect("two sides"),
    };
    let mediums = release_of(&[record, record]);
    let places = [(1, "A"), (1, "B"), (2, "C"), (2, "D")];
    let infos: Vec<_> = places
        .iter()
        .enumerate()
        .map(|(index, (medium, letter))| {
            info_on(&format!("track-{index}"), &mediums, *medium, sided(letter))
        })
        .collect();
    let labels: Vec<_> = infos
        .windows(2)
        .map(|pair| side_boundary_between(&pair[0], &pair[1]).expect("a side ends").ended)
        .collect();
    let heading = |letter: &str| SideHeading::Sided { side_letter: letter.to_string() };
    assert_eq!(labels, [heading("A"), heading("B"), heading("C")]);
}

/// A medium of no physical carrier never pauses.
#[test]
fn a_medium_of_no_carrier_never_pauses() {
    let unstated = bae_types::mediums::ReleaseMedium::one_sided(None);
    let mediums = release_of(&[unstated, unstated]);
    let disc = |disc| bae_types::mediums::TrackSide::Disc { disc };
    let first = info_on("first", &mediums, 1, disc(1));
    let second = info_on("second", &mediums, 2, disc(2));
    assert_eq!(first.side, None);
    assert_eq!(side_boundary_between(&first, &second), None);
}
