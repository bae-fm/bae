//! The one place bae takes a builder from a coven installation.

/// A builder over `installation` that reads `clock` and writes ids from
/// `ids`: the pair one process makes once, where the app's composition makes
/// it, so every library the process builds, opens or restores stamps and
/// names its rows the same way. Taking a builder any other way would leave
/// coven's own random ids in a library the app writes; the
/// `coven-builder-boundary` check holds every `.builder()` call in bae to
/// this function.
pub fn store_builder(
    installation: &coven::Coven,
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
) -> coven::CovenBuilder {
    installation.builder().clock(clock).id_source(ids)
}
