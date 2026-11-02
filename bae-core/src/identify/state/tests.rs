/// [`super::step`], answering each read of the offered records' documents at
/// once with none to be had, and each lookup of an offered row's pressing on
/// another catalog with nothing found: these tests are about the folder's
/// own keys, and a run whose documents and pressings add nothing settles on
/// what those found. The documents' and the pressings' own tests drive
/// `ReleasesRead` and `PressingLookupAnswered` themselves.
use crate::catalog_model::records::LookupRef;
fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    let (mut state, mut effects) = super::step(state, event);
    while let Some(at) = effects.iter().position(|effect| {
        matches!(effect, Effect::ReadReleases { .. } | Effect::LookupPressing { .. })
    }) {
        let answer = match effects.remove(at) {
            Effect::ReadReleases { releases, .. } => IdentifyEvent::ReleasesRead {
                read: releases
                    .into_iter()
                    .map(|release| crate::identify::documents::ReleaseReading {
                        release: release.release().clone(),
                        document: Err(LookupFailure::Network),
                    })
                    .collect(),
            },
            Effect::LookupPressing { source, key } => {
                IdentifyEvent::PressingLookupAnswered { source, key, outcome: Ok(Vec::new()) }
            }
            _ => unreachable!("the position is of a read or a pressing lookup"),
        };
        let (next, more) = super::step(state, answer);
        state = next;
        effects.extend(more);
    }
    (state, effects)
}

/// Whether a lookup of the folder's disc IDs found `result`, as the run's
/// ledger records it.
fn found_by_disc_id(
    ledger: Option<&crate::identify::IdentifyRunView>,
    result: &MetadataResult,
) -> bool {
    ledger.is_some_and(|ledger| {
        ledger.disc_id_releases().contains(&crate::catalog_model::records::LookupRef::new(
            result.source,
            &result.release_id,
        ))
    })
}

/// The records a read of documents asks for.
fn records_read(releases: &[crate::identify::documents::ReleaseToRead]) -> Vec<LookupRef> {
    releases.iter().map(|release| release.release().clone()).collect()
}

include!("tests/signals_and_conflicts.rs");
include!("tests/run_inputs.rs");
include!("tests/toolbar.rs");
include!("tests/title_search.rs");
include!("tests/album_links.rs");
include!("tests/documents.rs");
include!("tests/isrcs.rs");
include!("tests/track_titles.rs");
include!("tests/rounds.rs");
