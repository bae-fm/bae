//! How soon a request wants its picture.

/// A request's turn, which changes while the request waits when what the
/// person looks at changes. A later variant's picture is read first, so the
/// order of the variants is the order of the turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PictureTurn {
    /// No screen shows the picture yet, and one may soon.
    Prefetch,
    /// A screen shows the picture.
    Visible,
    /// The picture is the one the person looks at, among the several a screen
    /// shows: the one it shows large.
    Focused,
}
