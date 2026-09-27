/// [`super::step`], answering a read of the offered records' documents at
/// once with none: these tests are about the lookups, and a run whose
/// documents add nothing settles on what the lookups found. The documents'
/// own tests drive `ReleasesRead` themselves.
fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    let (state, mut effects) = super::step(state, event);
    let Some(read) = effects
        .iter()
        .position(|effect| matches!(effect, Effect::ReadReleases { .. }))
    else {
        return (state, effects);
    };
    effects.remove(read);
    let (state, more) = super::step(state, IdentifyEvent::ReleasesRead { read: Vec::new() });
    effects.extend(more);
    (state, effects)
}

include!("tests/signals_and_conflicts.rs");
include!("tests/run_inputs.rs");
include!("tests/toolbar.rs");
include!("tests/title_search.rs");
include!("tests/album_links.rs");
include!("tests/switched_off_steps.rs");
include!("tests/documents.rs");
