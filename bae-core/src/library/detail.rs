//! A detail view's live read: one subscription whose id moves as the view
//! shows another album, release, artist, composer, or work.

/// What a detail read delivered: the id it was read for, and that item's
/// detail, or `None` once no such item exists. A read for no id delivers
/// neither.
#[derive(Debug, Clone)]
pub struct DetailSnapshot<Value, Id = String> {
    pub id: Option<Id>,
    pub value: Option<Value>,
}

/// A detail view's live read. Its id changes in place through
/// [`LiveRead::set`](super::LiveRead::set); `None` reads nothing. An import
/// candidate's pane is read by the candidate's key.
pub type DetailSubscription<Value, Id = String> =
    super::LiveRead<Option<Id>, DetailSnapshot<Value, Id>>;
