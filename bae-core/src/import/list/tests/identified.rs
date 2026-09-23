//! The identified set: the Ready rows whose draft was read from a catalog's
//! release, which importing only identified rows acts on.

use super::*;
use crate::import::MetadataAuthor;

fn summary() -> Option<crate::import::TriageMetadataSummary> {
    Some(crate::import::TriageMetadataSummary {
        album_title: "Album".to_string(),
        album_artist_assignments: Vec::new(),
    })
}

/// A person's pick and identification's settled pick are both read from a
/// release and both join the set; a Ready draft the tags seeded or a person
/// typed does not, though every one of them is Ready.
#[test]
fn only_ready_rows_read_from_a_release_are_identified() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Picked"),
        candidate("Settled"),
        candidate("Tagged"),
        candidate("Typed"),
        candidate("Asking"),
    ];
    rows.states.insert(
        "hash-Picked".to_string(),
        CandidateStateListRow {
            metadata_provenance: Some(external_release_seed("mb-picked")),
            metadata_author: MetadataAuthor::Person,
            metadata_draft_valid: true,
            metadata_summary: summary(),
            ..several_matches_state()
        },
    );
    rows.states.insert(
        "hash-Settled".to_string(),
        CandidateStateListRow {
            metadata_summary: summary(),
            ..ready_state("mb-1")
        },
    );
    rows.states.insert(
        "hash-Tagged".to_string(),
        CandidateStateListRow {
            metadata_summary: summary(),
            ..prefilled_from_tags_state()
        },
    );
    rows.states.insert(
        "hash-Typed".to_string(),
        CandidateStateListRow {
            metadata_provenance: None,
            metadata_author: MetadataAuthor::Person,
            metadata_summary: summary(),
            ..prefilled_from_tags_state()
        },
    );
    rows.states.insert(
        "hash-Asking".to_string(),
        CandidateStateListRow {
            metadata_summary: summary(),
            ..with_verdict(ready_state("mb-2"), |verdict| {
                verdict.summary.track_count = Some(10);
            })
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let keys = |refs: &[ReadyRowRef]| {
        let mut keys: Vec<_> = refs.iter().map(|row| row.candidate_key.clone()).collect();
        keys.sort();
        keys
    };
    assert_eq!(
        keys(&flat.summary.ready),
        vec![key("Picked"), key("Settled"), key("Tagged"), key("Typed")]
    );
    assert_eq!(
        keys(&flat.summary.identified),
        vec![key("Picked"), key("Settled")]
    );
}

/// The set follows the view's filter the way the Ready set does.
#[test]
fn the_identified_set_follows_the_filter() {
    let mut rows = queue();
    rows.candidates = vec![candidate("First"), candidate("Second")];
    for name in ["First", "Second"] {
        rows.states.insert(
            format!("hash-{name}"),
            CandidateStateListRow {
                metadata_summary: summary(),
                ..ready_state("mb-1")
            },
        );
    }

    let flat = flattened(
        &rows,
        &ImportListView {
            filter_text: "second".to_string(),
            ..view(TriageTab::Pending)
        },
    );
    assert_eq!(
        flat.summary
            .identified
            .iter()
            .map(|row| row.candidate_key.clone())
            .collect::<Vec<_>>(),
        vec![key("Second")]
    );
}
