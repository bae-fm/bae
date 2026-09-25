//! A detail view's live read: one subscription whose id moves as the view
//! shows another album, release, artist, composer, or work.

/// What a detail read delivered: the id it was read for, and that item's
/// detail, or `None` once no such item exists. A read for no id delivers
/// neither.
#[derive(Debug, Clone)]
pub struct DetailSnapshot<Value> {
    pub id: Option<String>,
    pub value: Option<Value>,
}

/// A detail view's live read. Its id changes in place through
/// [`LiveRead::set`](super::LiveRead::set); `None` reads nothing.
pub type DetailSubscription<Value> = super::LiveRead<Option<String>, DetailSnapshot<Value>>;
