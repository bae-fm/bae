//! A count to show in words, as core names it and the UI words it.

/// A catalog key whose message takes `count`, and the count to render it with
/// (e.g. `core.queue.downloading` × 2): a part of a queue summary line, or the
/// count line under an artist's or composer's name. Core decides which keys
/// appear, in what order, and that a zero drops out of a summary; the UI
/// resolves each key against its catalog and joins the parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountLabel {
    pub key: String,
    pub count: u32,
}
