//! Why a lookup was not asked, carried by every step's "not asked" state.

/// Why a run did not ask about a value. When several hold, the one nearest
/// the value wins: a value left out says so even where no catalog answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NotAskedReason {
    /// The person left the value out of the run.
    LeftOut,
    /// No catalog the run asks answers this lookup (the disc ID's catalog is
    /// not among them).
    NoCatalog,
}
