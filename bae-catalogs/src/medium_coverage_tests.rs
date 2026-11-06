use super::*;

fn medium(count: usize) -> MediumShape {
    MediumShape { counts: BTreeSet::from([count]), carrier: None, holds_folder_disc: false }
}

fn positions(sets: Vec<MediumCoverage>) -> Vec<Vec<usize>> {
    sets.into_iter().map(|set| set.positions).collect()
}

#[test]
fn every_set_whose_counts_sum_to_the_folders_is_a_candidate() {
    let mediums = [medium(2), medium(1), medium(1), medium(3)];
    assert_eq!(positions(candidate_sets(&mediums, 3)), vec![vec![0, 1], vec![0, 2], vec![3]]);
}

/// A Discogs medium with an index counts both ways, so a set reaches the
/// folder's count through either layout.
#[test]
fn a_medium_with_several_layouts_reaches_the_folder_through_any() {
    let suite = MediumShape { counts: BTreeSet::from([10, 12]), ..medium(0) };
    let mediums = [suite, medium(10)];
    assert_eq!(positions(candidate_sets(&mediums, 10)), vec![vec![0], vec![1]]);
    assert_eq!(positions(candidate_sets(&mediums, 22)), vec![vec![0, 1]]);
}

#[test]
fn a_medium_that_lists_no_tracks_is_never_covered() {
    let empty = MediumShape { counts: BTreeSet::new(), ..medium(0) };
    let mediums = [empty, medium(2)];
    assert_eq!(positions(candidate_sets(&mediums, 2)), vec![vec![1]]);
}

#[test]
fn audio_no_set_holds_has_no_candidate() {
    assert!(candidate_sets(&[medium(2), medium(4)], 1).is_empty());
}

/// Past the exhaustive limit only consecutive runs are tried, and a run
/// is still found.
#[test]
fn a_large_box_offers_its_runs_of_discs() {
    let mediums: Vec<_> = (0..30).map(|_| medium(2)).collect();
    let sets = positions(candidate_sets(&mediums, 4));
    assert_eq!(sets.len(), 29);
    assert_eq!(sets[10], vec![10, 11]);
}

fn fit(shape: &[MediumShape], set: &[usize], origin: &AudioOrigin, stated: &[Option<u64>]) -> Fit {
    let measured: Vec<u64> = stated.iter().map(|_| 200_000).collect();
    Fit::of(shape, &MediumCoverage { positions: set.to_vec() }, origin, stated, &measured)
}

/// A medium that holds a folder's Disc ID decides over every length.
#[test]
fn a_disc_id_decides_before_lengths() {
    let holds = MediumShape { holds_folder_disc: true, ..medium(1) };
    let shape = [holds, medium(1)];
    let origin = AudioOrigin::default();
    assert!(
        fit(&shape, &[0], &origin, &[Some(260_000)]) < fit(&shape, &[1], &origin, &[Some(200_000)])
    );
}

/// A proven CD rip rules out a vinyl medium, whatever its lengths say,
/// and audio no CD plays rules out a CD.
#[test]
fn the_audio_origin_rules_out_carriers_before_lengths() {
    let vinyl = MediumShape { carrier: Some(Medium::Vinyl), ..medium(1) };
    let cd = MediumShape { carrier: Some(Medium::Cd), ..medium(1) };
    let shape = [vinyl, cd];
    let cd_rip = AudioOrigin {
        source: Some(crate::AudioSource::CdRip { proof: crate::CdProof::RipLog, file: None }),
        not_cd_rate: None,
    };
    assert!(fit(&shape, &[1], &cd_rip, &[None]) < fit(&shape, &[0], &cd_rip, &[Some(200_000)]));
    let high_rate = AudioOrigin { source: None, not_cd_rate: Some(96_000) };
    assert!(
        fit(&shape, &[0], &high_rate, &[None]) < fit(&shape, &[1], &high_rate, &[Some(200_000)])
    );
}

/// An unstated length never beats a stated one that agrees, and a stated
/// length that disagrees loses to an unstated one.
#[test]
fn lengths_count_disagreements_then_unstated_then_distance() {
    let shape = [medium(2), medium(2), medium(2)];
    let origin = AudioOrigin::default();
    let agreeing = fit(&shape, &[0], &origin, &[Some(200_500), Some(199_000)]);
    let unstated = fit(&shape, &[1], &origin, &[None, None]);
    let disagreeing = fit(&shape, &[2], &origin, &[Some(260_000), Some(200_000)]);
    assert!(agreeing < unstated);
    assert!(unstated < disagreeing);
    assert!(
        fit(&shape, &[0], &origin, &[Some(200_100), Some(200_000)])
            < fit(&shape, &[1], &origin, &[Some(201_000), Some(200_000)])
    );
}

#[test]
fn lengths_disagree_only_past_the_threshold_in_either_direction() {
    assert!(!lengths_disagree(180_000, 180_000));
    assert!(!lengths_disagree(180_000, 180_000 + LENGTH_DISAGREEMENT_MS));
    assert!(!lengths_disagree(180_000 + LENGTH_DISAGREEMENT_MS, 180_000));
    assert!(lengths_disagree(180_000, 180_001 + LENGTH_DISAGREEMENT_MS));
    assert!(lengths_disagree(180_001 + LENGTH_DISAGREEMENT_MS, 180_000));
}
