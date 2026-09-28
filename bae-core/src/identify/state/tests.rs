/// [`super::step`], answering each read of the offered records' documents at
/// once with none to be had: these tests are about the lookups, and a run
/// whose documents add nothing settles on what the lookups found. The
/// documents' own tests drive `ReleasesRead` themselves.
fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    let (mut state, mut effects) = super::step(state, event);
    while let Some(at) = effects
        .iter()
        .position(|effect| matches!(effect, Effect::ReadReleases { .. }))
    {
        let Effect::ReadReleases { releases, .. } = effects.remove(at) else {
            unreachable!("the position is of a read");
        };
        let read = releases
            .into_iter()
            .map(|release| crate::identify::documents::ReleaseReading {
                release,
                document: Err(LookupFailure::Network),
            })
            .collect();
        let (next, more) = super::step(state, IdentifyEvent::ReleasesRead { read });
        state = next;
        effects.extend(more);
    }
    (state, effects)
}

include!("tests/signals_and_conflicts.rs");
include!("tests/run_inputs.rs");
include!("tests/toolbar.rs");
include!("tests/title_search.rs");
include!("tests/album_links.rs");
include!("tests/switched_off_steps.rs");
include!("tests/documents.rs");
include!("tests/isrcs.rs");
