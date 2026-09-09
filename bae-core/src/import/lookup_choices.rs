//! What a person has decided one candidate's identification asks about, and
//! what it is to make of the answers.
//!
//! Extraction reads a folder's disc ID, its barcodes and its catalog numbers;
//! which of them a run actually looks up is a separate question, and the
//! answer is the person's. They uncheck a barcode that belongs to the box set
//! rather than the disc, or pick the one catalog number of thirty that names
//! this pressing — and that decision outlives the run they made it during.
//!
//! So it is stored with the candidate rather than held inside a running
//! reducer: a run reads it at its start, and every later run reads the same
//! value until the person changes it. Changing what a run looks up is what
//! starts the next one.

/// The signals one candidate's identification leaves out, the catalog numbers
/// it asks about, and the ones it is to read nothing into.
///
/// The default is what a candidate nobody has touched runs with: the disc ID
/// and the barcodes are asked about, no catalog number is — one number can
/// name thirty releases, so a number is looked up only once someone says it is
/// this disc's — and everything the folder's text says counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LookupChoices {
    /// Whether the run leaves the candidate's disc ID out.
    pub disc_id_excluded: bool,
    /// Whether the run leaves the candidate's barcodes out.
    pub barcode_excluded: bool,
    /// The catalog numbers the run looks up, each on its own, in the order
    /// they were chosen — which is the order their lookups are dispatched and
    /// their results laid out.
    pub chosen_catalogs: Vec<String>,
    /// The catalog numbers the folder's own text carries that the person
    /// struck out: a result whose catalog number is one of them agrees with
    /// the text about nothing, however plainly the text prints it.
    ///
    /// A set, each value once: nothing dispatches on their order, and a value
    /// is struck out or it is not. Nothing here reaches a provider — striking
    /// a number out changes how the answers in hand are ranked, not what was
    /// asked for them — so a number can be looked up and struck out at once.
    pub discounted_catalogs: Vec<String>,
}

impl LookupChoices {
    /// Whether these two ask the providers the same thing: the same signals
    /// and the same numbers. What the folder's text is taken to state about
    /// the answers is not part of it — that is read afresh every time the
    /// answers are, which is why changing it needs no run.
    pub fn asks_the_same_as(&self, other: &Self) -> bool {
        self.disc_id_excluded == other.disc_id_excluded
            && self.barcode_excluded == other.barcode_excluded
            && self.chosen_catalogs == other.chosen_catalogs
    }
}

/// What writing a candidate's choices changed — which is what says whether
/// the answers in hand still answer the question the person is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceChange {
    /// The lookups are different now, so what a run in hand found answers a
    /// question nobody is asking any more: the caller starts a run that reads
    /// the new value.
    Lookups,
    /// The same lookups, and only what the folder's text is taken to state
    /// about their answers is different. Nothing is asked again; the next
    /// read of the candidate ranks the stored answers by the new value.
    Ranking,
}
