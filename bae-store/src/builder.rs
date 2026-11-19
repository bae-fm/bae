//! The one place bae takes a builder from a coven installation.

/// A builder over `installation` that reads `clock` and writes ids from
/// `ids`: the pair one process makes once, where the app's composition makes
/// it, so every library the process builds, opens or restores stamps and
/// names its rows the same way. Taking a builder any other way would leave
/// coven's own random ids in a library the app writes; clippy
/// refuses every `Coven::builder` call in bae outside this function.
#[expect(
    clippy::disallowed_methods,
    reason = "coven builder: this is the one place bae takes a builder, with the clock and the ids together"
)]
pub fn store_builder(
    installation: &coven::Coven,
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
) -> coven::CovenBuilder {
    installation.builder().clock(clock).id_source(ids)
}
